//! The central service that sessions connect to.

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use riff_server::auth::Config;
use riff_server::gcs::Gcs;
use riff_server::oidc::{self, DEFAULT_DOMAIN, Provider, SignInError};
use riff_server::{Service, listen, service};
use tokio::signal::unix::{SignalKind, signal};

/// The central service that sessions connect to. With no command, it
/// runs in the foreground.
#[derive(Parser)]
#[command(version = riff_core::build::VERSION, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// The address to listen on.
    #[arg(
        long,
        env = "RIFF_LISTEN",
        default_value = "127.0.0.1:7878",
        global = true
    )]
    listen: SocketAddr,

    /// The verified email of a person who may revoke the tokens of any
    /// person. Repeat it for more admins.
    #[arg(
        long = "admin",
        env = "RIFF_ADMINS",
        value_delimiter = ',',
        global = true
    )]
    admins: Vec<String>,

    /// The URL where people reach the server. It is the OAuth resource
    /// and issuer. The default is http://<listen>.
    #[arg(long, env = "RIFF_PUBLIC_URL", global = true)]
    public_url: Option<String>,

    /// Refuse each request that has no live riff access token.
    #[arg(long, env = "RIFF_REQUIRE_SIGN_IN", global = true)]
    require_sign_in: bool,

    /// Listen on an address that is not loopback with no sign-in. Each
    /// machine that can reach the server can then read, post and answer
    /// as any person. riff-server has no TLS.
    #[arg(long, env = "RIFF_INSECURE", global = true)]
    insecure: bool,

    /// The OpenID Connect issuer that people sign in with.
    #[arg(
        long,
        env = "RIFF_OIDC_ISSUER",
        default_value = "https://accounts.google.com",
        global = true
    )]
    issuer: String,

    /// The OAuth client ID of riff at the issuer. Without it, the server
    /// has no sign-in.
    #[arg(long, env = "RIFF_OIDC_CLIENT_ID", global = true)]
    client_id: Option<String>,

    /// The client secret, when the issuer asks for one. Google asks for
    /// it for a desktop client. It is not a secret: each `riff login`
    /// gets it.
    #[arg(
        long,
        env = "RIFF_OIDC_CLIENT_SECRET",
        requires = "client_id",
        global = true
    )]
    client_secret: Option<String>,

    /// A Workspace domain whose accounts may sign in. Repeat it for more
    /// domains.
    #[arg(
        long = "allowed-domain",
        env = "RIFF_ALLOWED_DOMAINS",
        value_delimiter = ',',
        default_value = DEFAULT_DOMAIN,
        global = true
    )]
    allowed_domains: Vec<String>,

    /// The Cloud Storage bucket that holds the state. The server loads
    /// the state at start and saves each change. Without it, the server
    /// saves nothing.
    #[arg(long, env = "RIFF_BUCKET", global = true)]
    bucket: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Install riff-server as a systemd user service with these
    /// settings, and start it. Run it again to update the service: it
    /// keeps each old setting that it does not get again.
    Install {
        /// The systemctl command.
        #[arg(long, default_value = "systemctl")]
        systemctl: PathBuf,
    },
    /// Stop the systemd user service and remove it.
    Uninstall {
        /// The systemctl command.
        #[arg(long, default_value = "systemctl")]
        systemctl: PathBuf,
    },
}

impl Cli {
    /// The settings as environment variables, for the service.
    fn settings(&self) -> Vec<(&'static str, String)> {
        let mut settings = vec![
            ("RIFF_LISTEN", self.listen.to_string()),
            ("RIFF_OIDC_ISSUER", self.issuer.clone()),
            ("RIFF_ALLOWED_DOMAINS", self.allowed_domains.join(",")),
        ];
        if !self.admins.is_empty() {
            settings.push(("RIFF_ADMINS", self.admins.join(",")));
        }
        if let Some(url) = &self.public_url {
            settings.push(("RIFF_PUBLIC_URL", url.clone()));
        }
        if self.require_sign_in {
            settings.push(("RIFF_REQUIRE_SIGN_IN", "true".into()));
        }
        if self.insecure {
            settings.push(("RIFF_INSECURE", "true".into()));
        }
        if let Some(id) = &self.client_id {
            settings.push(("RIFF_OIDC_CLIENT_ID", id.clone()));
        }
        if let Some(secret) = &self.client_secret {
            settings.push(("RIFF_OIDC_CLIENT_SECRET", secret.clone()));
        }
        if let Some(bucket) = &self.bucket {
            settings.push(("RIFF_BUCKET", bucket.clone()));
        }
        settings
    }

    /// [`Cli::settings`] in two parts: the settings that the person
    /// gave, as an option or as a variable, and the defaults.
    fn given_settings(&self, matches: &ArgMatches) -> Given {
        let command = Cli::command();
        let (given, defaults) = self.settings().into_iter().partition(|(name, _)| {
            command
                .get_arguments()
                .find(|a| a.get_env().is_some_and(|e| e == *name))
                .and_then(|a| matches.value_source(a.get_id().as_str()))
                .is_some_and(|source| source != ValueSource::DefaultValue)
        });
        Given { given, defaults }
    }

    /// True for a riff with no sign-in (R211).
    fn trusted(&self) -> bool {
        self.client_id.is_none() && !self.require_sign_in
    }
}

/// The settings of an install, see [`Cli::given_settings`].
struct Given {
    given: Vec<(&'static str, String)>,
    defaults: Vec<(&'static str, String)>,
}

/// Checks the listen address of the settings of a service, as
/// `riff-server` checks its own at start (01M3JCE4ZD4DZCQ21FA69RT52D).
fn check_settings(settings: &[(String, String)]) -> std::io::Result<(SocketAddr, Option<String>)> {
    let get = |name: &str| {
        settings
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let on = |name: &str| get(name).is_some_and(truthy);
    let listen: SocketAddr = get("RIFF_LISTEN")
        .unwrap_or("127.0.0.1:7878")
        .parse()
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("RIFF_LISTEN: {e}"),
            )
        })?;
    let trusted =
        get("RIFF_OIDC_CLIENT_ID").is_none_or(str::is_empty) && !on("RIFF_REQUIRE_SIGN_IN");
    let warning = listen::check(listen, trusted, on("RIFF_INSECURE"))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    Ok((listen, warning))
}

/// True for a flag value that clap reads as set.
fn truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "n" | "no" | "f" | "false" | "off"
    )
}

/// Installs or removes the service, and says what it did.
fn manage(cli: &Cli, matches: &ArgMatches, command: &Command) -> std::io::Result<()> {
    let dir = service::dir()?;
    match command {
        Command::Install { systemctl } => {
            let exe = std::env::current_exe()?;
            let Given { given, defaults } = cli.given_settings(matches);
            let settings = service::merge(service::installed(&dir)?, &given, &defaults);
            let (listen, warning) = check_settings(&settings)?;
            service::install(systemctl, &dir, &exe, &settings)?;
            println!(
                "Installed riff-server as a systemd user service. It listens on {}.\n\
                 Unit: {}\n\
                 Settings: {}\n\
                 Status: systemctl --user status riff-server\n\
                 Logs: journalctl --user -u riff-server\n\
                 To keep it running after you log out: loginctl enable-linger",
                listen,
                dir.join(service::UNIT).display(),
                dir.join(service::ENV).display()
            );
            if let Some(warning) = warning {
                println!("Warning: {warning}.");
            }
        }
        Command::Uninstall { systemctl } => {
            if service::uninstall(systemctl, &dir)? {
                println!("Removed the riff-server service.");
            } else {
                println!("The riff-server service is not installed.");
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("riff-server: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> std::io::Result<()> {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    if let Some(command) = &cli.command {
        return manage(&cli, &matches, command);
    }
    let warning = listen::check(cli.listen, cli.trusted(), cli.insecure)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    if let Some(warning) = warning {
        tracing::warn!("{warning}");
    }
    // Catch SIGTERM before the server says that it listens.
    let mut terminate = signal(SignalKind::terminate())?;
    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    let public_url = cli
        .public_url
        .unwrap_or_else(|| format!("http://{}", listener.local_addr().unwrap_or(cli.listen)));
    let mut config = Config::new(&public_url);
    config.require_sign_in = cli.require_sign_in;
    config.admins = cli.admins;
    for admin in &config.admins {
        if !admin.contains('@') {
            tracing::warn!("the admin {admin} is not an email: it names nobody (R210)");
        }
    }
    tracing::info!("riff-server listens on {}", listener.local_addr()?);
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
    let service = match &cli.bucket {
        Some(bucket) => {
            tracing::info!("state in gs://{bucket}");
            let store = Arc::new(Gcs::new(bucket));
            Service::load(config, store)
                .await
                .map_err(std::io::Error::other)?
        }
        None => {
            tracing::warn!("no RIFF_BUCKET: the state is not saved");
            Service::new(config)
        }
    };
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
    use super::{Cli, Given, check_settings};
    use clap::{CommandFactory, FromArgMatches, Parser};

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn each_setting_is_an_env_of_the_cli() {
        let cli = Cli::parse_from([
            "riff-server",
            "--admin=a@comotechnologies.io",
            "--public-url=https://x",
            "--require-sign-in",
            "--insecure",
            "--client-id=id",
            "--client-secret=s",
            "--bucket=b",
        ]);
        let envs: Vec<String> = Cli::command()
            .get_arguments()
            .filter_map(|a| a.get_env().map(|e| e.to_string_lossy().into_owned()))
            .collect();
        let settings = cli.settings();
        assert_eq!(settings.len(), envs.len());
        for (name, _) in settings {
            assert!(envs.iter().any(|e| e == name), "{name}");
        }
    }

    #[test]
    fn a_setting_is_given_by_an_option_not_by_a_default() {
        let args = ["riff-server", "install", "--listen=0.0.0.0:1", "--insecure"];
        let matches = Cli::command().get_matches_from(args);
        let cli = Cli::from_arg_matches(&matches).unwrap();
        let Given { given, defaults } = cli.given_settings(&matches);
        let names = |s: &[(&'static str, String)]| s.iter().map(|(n, _)| *n).collect::<Vec<_>>();
        assert_eq!(names(&given), ["RIFF_LISTEN", "RIFF_INSECURE"]);
        assert!(names(&defaults).contains(&"RIFF_OIDC_ISSUER"));
    }

    #[test]
    fn check_settings_reads_the_settings_file() {
        let s = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect::<Vec<_>>()
        };
        let open = ("RIFF_LISTEN", "0.0.0.0:7878");
        assert!(check_settings(&s(&[open])).is_err());
        assert!(check_settings(&s(&[open, ("RIFF_INSECURE", "false")])).is_err());
        let (_, warning) = check_settings(&s(&[open, ("RIFF_INSECURE", "true")])).unwrap();
        assert!(warning.is_some());
        let (_, warning) = check_settings(&s(&[open, ("RIFF_OIDC_CLIENT_ID", "id")])).unwrap();
        assert!(warning.is_none());
        let (listen, _) = check_settings(&[]).unwrap();
        assert_eq!(listen.to_string(), "127.0.0.1:7878");
    }
}
