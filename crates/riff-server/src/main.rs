//! The central service that sessions connect to.

use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use riff_server::auth::Config;
use riff_server::gcs::Gcs;
use riff_server::keys::At;
use riff_server::lease::Revision;
use riff_server::listen::{self, Port, Start};
use riff_server::oidc::{self, DEFAULT_DOMAIN, Provider, SignInError};
use riff_server::{Service, logline, tools};
use riff_state::store::{Dir, Store};
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

    /// The verified email of an admin. The server makes the person an
    /// admin at each start. Repeat it for more admins. Without it, the
    /// first person who signs in is an admin, and the server listens
    /// only on a loopback address until then.
    #[arg(
        long = "admin",
        env = "RIFF_ADMINS",
        hide_env_values = true,
        value_delimiter = ','
    )]
    admins: Vec<String>,

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

    /// The secret of Secret Manager that holds the keys of the GitHub
    /// Apps of the riff, projects/PROJECT/secrets/NAME. riff cloud deploy
    /// sets it. Without it, the keys are in the file github-apps.json of
    /// --dir, or in memory. With a bucket and no secret, riff github
    /// create refuses.
    #[arg(long, env = "RIFF_GITHUB_SECRET", hide_env_values = true)]
    github_secret: Option<String>,
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
    /// Print each record and each checkpoint after a position. With
    /// --yes, delete them. Stop the server first: with --yes, the command
    /// refuses while a server holds the lease. It refuses a position
    /// before the oldest kept checkpoint.
    Cut {
        /// The last position that stays.
        #[arg(long, value_name = "POSITION")]
        after: u64,

        /// Delete the records and the checkpoints. Without it, the
        /// command only prints them.
        #[arg(long)]
        yes: bool,
    },
}

impl Cli {
    /// True for a riff with no sign-in (R211).
    fn trusted(&self) -> bool {
        self.client_id.is_none() && !self.require_sign_in
    }

    /// Where the server keeps the keys of the GitHub Apps
    /// (01M6WG52WSWM8A9CN6X448AGAJ): the secret, else the folder of the
    /// state. A bucket with no secret has no place: the keys never go
    /// into the bucket.
    fn keys(&self) -> At {
        match (&self.github_secret, &self.bucket, &self.dir) {
            (Some(name), _, _) => At::secret(name),
            (None, Some(_), _) => At::Nowhere,
            (None, None, Some(dir)) => At::Dir(dir.clone()),
            (None, None, None) => At::Memory,
        }
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
        Some(Command::Log(args)) => {
            // A closed pipe on stdout ends the tool quietly. The server
            // keeps the default hook (01M5MRG35BYRGZYD4VQVV8ZTYX).
            riff_core::pipe::end_quietly();
            log_tool(&cli, args).await
        }
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
        Some(LogAction::Cut { after, yes }) => {
            // Without --yes, a dry run (01M3X342G8KF2W06PABGXTERMZ).
            let mode = if *yes {
                tools::Mode::Remove
            } else {
                tools::Mode::DryRun
            };
            let removed = tools::cut(&*store, *after, mode).await.map_err(told)?;
            for line in &removed.records {
                println!("{line}");
            }
            for name in &removed.checkpoints {
                println!("checkpoint  {}", store.locate(name));
            }
            println!("{}", tools::cut_text(&removed, *after, mode));
            // A server took the lease during the cut: the text says so.
            if *yes && removed.held.is_some() {
                return Err(Stop::Logged);
            }
            Ok(())
        }
    }
}

/// Runs the server until it stops.
async fn run(cli: Cli) -> Result<(), Stop> {
    // The bucket can hold an admin: check again after the load.
    let trusted = cli.trusted();
    let admin = cli.admins.iter().any(|a| a.contains('@')) || cli.bucket.is_some();
    let warning = listen::check(cli.listen, trusted, cli.insecure, admin).map_err(Stop::Told)?;
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
    // The port opens only after the load (01M52JYGHHK5X0XS39VFT3ZGGM).
    let port = Port::reserve(cli.listen)?;
    let keys = cli.keys();
    let public_url = cli
        .public_url
        .unwrap_or_else(|| format!("http://{}", port.addr()));
    let mut config = Config::new(&public_url);
    // A riff with a provider requires sign-in (01M3JZN1XQVVNVD0MJVM8J91HC).
    config.require_sign_in = cli.require_sign_in || cli.client_id.is_some();
    config.github.keys = keys;
    tracing::info!(
        "the keys of the GitHub Apps: in {}",
        config.github.keys.describe()
    );
    config.admins = cli.admins;
    // Cloud Run names the revision of the instance: the order of the
    // deploys (01M53NHJXGK1PN2A9F7NSX0B4G).
    if let Ok(name) = std::env::var("K_REVISION") {
        config.revision = Revision::parse(&name);
        match config.revision {
            Some(revision) => tracing::info!("the revision {name}: the deploy {revision}"),
            None => tracing::warn!("the revision {name} has no number: the lease holds none"),
        }
    }
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
    let start = match store {
        Some(store) => match Service::prepare(config, store).await {
            Ok(loaded) => Start::AfterLease(Box::new(loaded)),
            Err(e) => {
                // The text names the object and the fix
                // (01M3MMXYS1V8CA89D2XHKPR6C4).
                tracing::error!("riff-server stops: {}", e.at_start());
                std::process::exit(1);
            }
        },
        None => {
            tracing::warn!("no RIFF_BUCKET and no RIFF_DIR: the log is in memory only");
            Start::Now(Service::new(config))
        }
    };
    // The admins are in the state: check the address again before the
    // port opens, and after the first save.
    if let Start::AfterLease(loaded) = &start {
        listen::check(cli.listen, trusted, cli.insecure, loaded.has_admin())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    }
    let stop = async move {
        tokio::select! {
            _ = terminate.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    };
    let (listen, insecure) = (cli.listen, cli.insecure);
    listen::serve(port, start, stop, |service| {
        listen::check(listen, trusted, insecure, service.has_admin()).map(|_| ())
    })
    .await
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
        Err(e @ (SignInError::Provider(_) | SignInError::Invalid(_))) => {
            tracing::warn!("cannot check the OAuth client: {}", e.for_log());
        }
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

    /// 01M77671TQ92487GSM24RJG82Z: riff-server has no setting of the owner
    /// role. `--admin` names each admin, also from `RIFF_ADMINS` with commas.
    #[test]
    fn the_admins_are_the_only_setting_of_the_roles() {
        use clap::Parser;
        for old in [
            "--owner",
            "--owner-take-minutes",
            "--owner-ping-minutes",
            "--owner-pings",
        ] {
            assert!(
                Cli::try_parse_from(["riff-server", old, "1"]).is_err(),
                "{old}"
            );
        }
        let cli = Cli::try_parse_from(["riff-server", "--admin", "a@x.io,b@x.io"]).unwrap();
        assert_eq!(cli.admins, ["a@x.io", "b@x.io"]);
    }

    /// 01M6WG52WSWM8A9CN6X448AGAJ: the keys of the GitHub Apps go to the
    /// secret, else to the folder of the state. A bucket with no secret
    /// has no place for them.
    #[test]
    fn the_keys_of_the_github_apps_go_to_the_secret_or_the_folder() {
        use clap::Parser;
        use riff_server::keys::At;
        let keys = |args: &[&str]| Cli::try_parse_from(args).unwrap().keys();
        assert_eq!(keys(&["riff-server"]), At::Memory);
        assert_eq!(keys(&["riff-server", "--dir", "d"]), At::Dir("d".into()));
        assert_eq!(keys(&["riff-server", "--bucket", "b"]), At::Nowhere);
        let name = "projects/p/secrets/s";
        assert_eq!(
            keys(&["riff-server", "--bucket", "b", "--github-secret", name]),
            At::secret(name)
        );
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

    /// 01M3X342G8KF2W06PABGXTERMZ: a cut removes only with `--yes`.
    #[test]
    fn a_cut_is_a_dry_run_with_no_yes() {
        use super::{Command, LogAction};
        use clap::Parser;
        let yes = |args: &[&str]| {
            let cli = Cli::try_parse_from(args).unwrap();
            let Some(Command::Log(log)) = cli.command else {
                panic!("not the log command");
            };
            let Some(LogAction::Cut { after, yes }) = log.action else {
                panic!("not a cut");
            };
            assert_eq!(after, 7);
            yes
        };
        let cut = ["riff-server", "log", "cut", "--after", "7", "--dir", "d"];
        assert!(!yes(&cut));
        let cut = ["riff-server", "log", "cut", "--after", "7", "--yes"];
        assert!(yes(&cut));
    }
}
