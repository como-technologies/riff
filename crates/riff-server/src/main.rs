//! The central service that sessions connect to.

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use riff_server::auth::Config;
use riff_server::gcs::Gcs;
use riff_server::listen::{self, Port};
use riff_server::oidc::{self, DEFAULT_DOMAIN, Provider, SignInError};
use riff_server::store::{Dir, Store};
use riff_server::{Service, logline, tools};
use tokio::signal::unix::{SignalKind, signal};

/// The central service that sessions connect to. With no command, it
/// runs in the foreground (R118).
#[derive(Parser)]
#[command(version = riff_core::build::VERSION, about, max_term_width = 80)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// The address to listen on.
    #[arg(
        long,
        env = "RIFF_LISTEN",
        hide_env_values = true,
        default_value = "127.0.0.1:7878"
    )]
    listen: SocketAddr,

    /// The verified email of a person who may revoke the tokens of any
    /// person. Repeat it for more admins.
    #[arg(
        long = "admin",
        env = "RIFF_ADMINS",
        hide_env_values = true,
        value_delimiter = ','
    )]
    admins: Vec<String>,

    /// The verified email of the owner of a new riff. Without it, the
    /// first person who signs in is the owner, and the server listens
    /// only on a loopback address until then. A riff that has an owner
    /// keeps it.
    #[arg(long, env = "RIFF_OWNER", hide_env_values = true)]
    owner: Option<String>,

    /// The URL where people reach the server. It is the OAuth resource
    /// and issuer. The default is http://<listen>.
    #[arg(long, env = "RIFF_PUBLIC_URL", hide_env_values = true)]
    public_url: Option<String>,

    /// Refuse each request that has no live riff access token.
    #[arg(long, env = "RIFF_REQUIRE_SIGN_IN", hide_env_values = true)]
    require_sign_in: bool,

    /// Listen on an address that is not loopback with no sign-in. Each
    /// machine that can reach the server can then read, post and answer
    /// as any person. riff-server has no TLS.
    #[arg(long, env = "RIFF_INSECURE", hide_env_values = true)]
    insecure: bool,

    /// The OpenID Connect issuer that people sign in with.
    #[arg(
        long,
        env = "RIFF_OIDC_ISSUER",
        hide_env_values = true,
        default_value = "https://accounts.google.com"
    )]
    issuer: String,

    /// The OAuth client ID of your own OIDC app at the issuer. With it,
    /// each call needs sign-in. Without it, the server has no sign-in.
    /// riff has no built-in client.
    #[arg(long, env = "RIFF_OIDC_CLIENT_ID", hide_env_values = true)]
    client_id: Option<String>,

    /// The client secret, when the issuer asks for one. Google asks for
    /// it for a desktop client. It is not a secret: each `riff login`
    /// gets it.
    #[arg(
        long,
        env = "RIFF_OIDC_CLIENT_SECRET",
        hide_env_values = true,
        requires = "client_id"
    )]
    client_secret: Option<String>,

    /// A Workspace domain whose accounts may sign in. Repeat it for more
    /// domains.
    #[arg(
        long = "allowed-domain",
        env = "RIFF_ALLOWED_DOMAINS", hide_env_values = true,
        value_delimiter = ',',
        default_value = DEFAULT_DOMAIN
    )]
    allowed_domains: Vec<String>,

    /// The Cloud Storage bucket that holds the state. The server writes
    /// each change to the log in the bucket, and replays the log at
    /// start. Without a bucket or a directory, the server keeps its log in
    /// memory, and a restart loses it.
    #[arg(
        long,
        env = "RIFF_BUCKET",
        hide_env_values = true,
        conflicts_with = "dir",
        global = true
    )]
    bucket: Option<String>,

    /// A directory that holds the state as files, for local development:
    /// the log, the token store and the lease. The server replays the log
    /// at start.
    #[arg(long, env = "RIFF_DIR", hide_env_values = true, global = true)]
    dir: Option<std::path::PathBuf>,

    /// The minutes that the owner has to answer `riff owner --take` of an
    /// admin. With no answer, the admin is the owner.
    #[arg(long, env = "RIFF_OWNER_TAKE_MINUTES", hide_env_values = true, default_value_t = 10,
          value_parser = clap::value_parser!(u64).range(1..))]
    owner_take_minutes: u64,

    /// The minutes between two checks of the owner. A check misses when
    /// no session of the owner is live, and the owner made no call since
    /// the last check.
    #[arg(long, env = "RIFF_OWNER_PING_MINUTES", hide_env_values = true, default_value_t = 10,
          value_parser = clap::value_parser!(u64).range(1..))]
    owner_ping_minutes: u64,

    /// The misses in a row after which the server warns the owner. One
    /// more miss, and the owner is gone: the riff then has no owner, and
    /// asks each admin for a volunteer.
    #[arg(long, env = "RIFF_OWNER_PINGS", hide_env_values = true, default_value_t = 3,
          value_parser = clap::value_parser!(u32).range(1..))]
    owner_pings: u32,
}

/// The commands of `riff-server`. It runs no server for them.
#[derive(Subcommand)]
enum Command {
    /// Print the records of the log as text, one record on a line. Name
    /// the store with --bucket or --dir. With a bucket, the token comes
    /// from the metadata server of Cloud Run, or from your Google sign-in
    /// (gcloud auth login).
    Log(LogArgs),
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct LogArgs {
    /// Print the records from this position.
    #[arg(long, default_value_t = 1, value_name = "POSITION")]
    from: u64,

    #[command(subcommand)]
    action: Option<LogAction>,
}

#[derive(Subcommand)]
enum LogAction {
    /// Read each checkpoint, and each chunk of the log from the oldest
    /// kept checkpoint. Name each line that does not read, and each gap in
    /// the positions. Exit with 1 when it finds a problem.
    Verify,
    /// Delete each record and each checkpoint after a position, and print
    /// what it removes. Stop the server first. It refuses a position
    /// before the oldest kept checkpoint.
    Cut {
        /// The last position that stays.
        #[arg(long, value_name = "POSITION")]
        after: u64,
    },
}

impl Cli {
    /// True for a riff with no sign-in (R211).
    fn trusted(&self) -> bool {
        self.client_id.is_none() && !self.require_sign_in
    }
}

/// Why `riff-server` stops with a failure.
enum Stop {
    /// An error before the log starts, or of a command: `main` prints
    /// it.
    Told(String),
    /// The log has the error, or the command printed it.
    Logged,
}

impl From<std::io::Error> for Stop {
    fn from(error: std::io::Error) -> Self {
        Stop::Told(error.to_string())
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let run = match &cli.command {
        Some(Command::Log(args)) => log_tool(&cli, args).await,
        None => run(cli).await,
    };
    match run {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(Stop::Told(e)) => {
            eprintln!("riff-server: {e}");
            std::process::ExitCode::FAILURE
        }
        Err(Stop::Logged) => std::process::ExitCode::FAILURE,
    }
}

/// Runs a tool of the log on the store that the options name
/// (01M3TJWHNYRCA7RTPFNYM5ZNQS). See [`tools`].
async fn log_tool(cli: &Cli, args: &LogArgs) -> Result<(), Stop> {
    let store: Box<dyn Store> = match (&cli.bucket, &cli.dir) {
        (Some(bucket), _) => Box::new(Gcs::for_person(bucket)),
        (None, Some(dir)) => Box::new(Dir::new(dir)),
        (None, None) => {
            return Err(Stop::Told(
                "name the store of the log: --bucket BUCKET or --dir DIR".into(),
            ));
        }
    };
    let told = |e: tools::ToolError| Stop::Told(e.to_string());
    match &args.action {
        None => {
            tools::print(&*store, args.from, &mut |line| println!("{line}"))
                .await
                .map_err(told)?;
            Ok(())
        }
        Some(LogAction::Verify) => {
            let verified = tools::verify(&*store).await.map_err(told)?;
            for problem in &verified.problems {
                println!("{problem}");
            }
            println!("{}", tools::verified_text(&verified));
            if verified.problems.is_empty() {
                Ok(())
            } else {
                Err(Stop::Logged)
            }
        }
        Some(LogAction::Cut { after }) => {
            let removed = tools::cut(&*store, *after).await.map_err(told)?;
            for line in &removed.records {
                println!("{line}");
            }
            println!("{}", tools::cut_text(&removed, *after));
            Ok(())
        }
    }
}

/// Runs the server until it stops.
async fn run(cli: Cli) -> Result<(), Stop> {
    if let Some(owner) = cli.owner.as_deref().filter(|o| !o.contains('@')) {
        return Err(Stop::Told(format!("the owner {owner} is not an email")));
    }
    // The bucket can hold an owner: check again after the load.
    let trusted = cli.trusted();
    let owned = cli.owner.is_some() || cli.bucket.is_some();
    let warning = listen::check(cli.listen, trusted, cli.insecure, owned).map_err(Stop::Told)?;
    // Each line of the log is JSON with a severity
    // (01M3TJWJ3VK671T9NM95F3ES82).
    logline::init();
    if let Some(warning) = warning {
        tracing::warn!("{warning}");
    }
    serve(cli, trusted).await.map_err(|error| {
        tracing::error!("riff-server stops: {error}");
        Stop::Logged
    })
}

/// Loads the state, opens the port and serves. The log runs.
async fn serve(cli: Cli, trusted: bool) -> std::io::Result<()> {
    // Catch SIGTERM before the server says that it listens.
    let mut terminate = signal(SignalKind::terminate())?;
    // The port opens only after the load (01M3THEE31H5QVV3JAFC4ZRGFR).
    let port = Port::reserve(cli.listen)?;
    let public_url = cli
        .public_url
        .unwrap_or_else(|| format!("http://{}", port.addr()));
    let mut config = Config::new(&public_url);
    // A riff with a provider requires sign-in (01M3JZN1XQVVNVD0MJVM8J91HC).
    config.require_sign_in = cli.require_sign_in || cli.client_id.is_some();
    config.admins = cli.admins;
    config.owner = cli.owner;
    config.owner_role = riff_server::owner::Timing::from_minutes(
        cli.owner_take_minutes,
        cli.owner_ping_minutes,
        cli.owner_pings,
    );
    for admin in &config.admins {
        if !admin.contains('@') {
            tracing::warn!("the admin {admin} is not an email: it names nobody (R210)");
        }
    }
    if let Some(client_id) = cli.client_id {
        tracing::info!("sign-in with {}", cli.issuer);
        let provider = Provider {
            issuer: cli.issuer,
            client_id,
            client_secret: cli.client_secret,
            allowed_domains: cli.allowed_domains,
        };
        check_client(&provider).await;
        config.provider = Some(provider);
    } else {
        tracing::warn!("no RIFF_OIDC_CLIENT_ID: nobody can sign in");
    }
    let store: Option<Arc<dyn Store>> = match (&cli.bucket, &cli.dir) {
        (Some(bucket), _) => {
            tracing::info!("state in gs://{bucket}");
            Some(Arc::new(Gcs::new(bucket)))
        }
        (None, Some(dir)) => {
            tracing::info!("state in {}", dir.display());
            Some(Arc::new(Dir::new(dir)))
        }
        (None, None) => None,
    };
    let service = match store {
        Some(store) => match Service::load(config, store).await {
            Ok(service) => service,
            Err(e) => {
                // The text names the object and the fix
                // (01M3MMXYS1V8CA89D2XHKPR6C4).
                tracing::error!("riff-server stops: {e}");
                std::process::exit(1);
            }
        },
        None => {
            tracing::warn!("no RIFF_BUCKET and no RIFF_DIR: the log is in memory only");
            Service::new(config)
        }
    };
    let owned = service.tokens().owned();
    listen::check(cli.listen, trusted, cli.insecure, owned)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let listener = port.open()?;
    tracing::info!("riff-server listens on {}", listener.local_addr()?);
    let stop = async {
        tokio::select! {
            _ = terminate.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    };
    tokio::select! {
        result = axum::serve(listener, service.router()).into_future() => result,
        () = stop => {
            // Take no more calls, save each unsaved change, then exit (R129).
            tracing::info!("stopping: saving the state");
            service.shutdown().await.map_err(std::io::Error::other)
        }
        () = async {
            service.stopped().await;
            tokio::time::sleep(service.config().lease.exit_after).await;
        } => {
            // Another instance serves now (R140).
            tracing::info!("exiting: another instance serves");
            Ok(())
        }
    }
}

/// Stops the process when the provider refuses the OAuth client
/// (R146). When the provider does not answer, the server serves (R153).
async fn check_client(provider: &Provider) {
    match provider
        .check_client(&oidc::client(oidc::FETCH_TIMEOUT))
        .await
    {
        Ok(()) => tracing::info!("the provider knows the OAuth client"),
        Err(e @ SignInError::Client(_)) => {
            tracing::error!("riff-server stops: {e}");
            std::process::exit(1);
        }
        Err(e) => tracing::warn!("cannot check the OAuth client: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn the_owner_settings_set_the_window_of_the_owner_check() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["riff-server"]).unwrap();
        let timing = riff_server::owner::Timing::from_minutes(
            cli.owner_take_minutes,
            cli.owner_ping_minutes,
            cli.owner_pings,
        );
        assert_eq!(timing, riff_server::owner::Timing::default());
        let cli = Cli::try_parse_from([
            "riff-server",
            "--owner-ping-minutes",
            "20",
            "--owner-pings",
            "4",
        ])
        .unwrap();
        assert_eq!((cli.owner_ping_minutes, cli.owner_pings), (20, 4));
        assert!(Cli::try_parse_from(["riff-server", "--owner-pings", "0"]).is_err());
    }

    /// 01M3K0QM5HY852J4E5M2YQDYEM: the only command is `log`, with its
    /// tools `verify` and `cut`.
    #[test]
    fn the_only_command_is_log_with_verify_and_cut() {
        let cli = Cli::command();
        let names = |c: &clap::Command| -> Vec<String> {
            c.get_subcommands()
                .map(|s| s.get_name().to_owned())
                .collect()
        };
        assert_eq!(names(&cli), ["log"]);
        let log = cli.find_subcommand("log").unwrap();
        assert_eq!(names(log), ["verify", "cut"]);
    }

    #[test]
    fn the_log_tools_take_the_store_before_or_after_the_command() {
        use clap::Parser;
        for args in [
            &["riff-server", "--dir", "d", "log", "verify"][..],
            &["riff-server", "log", "verify", "--dir", "d"][..],
            &["riff-server", "log", "--from", "7", "--bucket", "b"][..],
            &["riff-server", "log", "cut", "--after", "7", "--dir", "d"][..],
        ] {
            assert!(Cli::try_parse_from(args).is_ok(), "{args:?}");
        }
        // A cut names its position.
        assert!(Cli::try_parse_from(["riff-server", "log", "cut", "--dir", "d"]).is_err());
    }
}
