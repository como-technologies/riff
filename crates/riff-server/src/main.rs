//! The central service that sessions connect to.

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use riff_server::auth::Config;
use riff_server::gcs::Gcs;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};
use riff_server::{Service, service};
use tokio::signal::unix::{SignalKind, signal};

/// The central service that sessions connect to. With no command, it
/// runs in the foreground.
#[derive(Parser)]
#[command(version, about)]
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

    /// A person who may revoke the tokens of any person. Repeat it for
    /// more admins.
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
    /// settings, and start it. Run it again to update the service.
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
}

/// Installs or removes the service, and says what it did.
fn manage(cli: &Cli, command: &Command) -> std::io::Result<()> {
    let dir = service::dir()?;
    match command {
        Command::Install { systemctl } => {
            let exe = std::env::current_exe()?;
            service::install(systemctl, &dir, &exe, &cli.settings())?;
            println!(
                "Installed riff-server as a systemd user service. It listens on {}.\n\
                 Unit: {}\n\
                 Settings: {}\n\
                 Status: systemctl --user status riff-server\n\
                 Logs: journalctl --user -u riff-server\n\
                 To keep it running after you log out: loginctl enable-linger",
                cli.listen,
                dir.join(service::UNIT).display(),
                dir.join(service::ENV).display()
            );
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
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    if let Some(command) = &cli.command {
        return manage(&cli, command);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    // Catch SIGTERM before the server says that it listens.
    let mut terminate = signal(SignalKind::terminate())?;
    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    let public_url = cli
        .public_url
        .unwrap_or_else(|| format!("http://{}", listener.local_addr().unwrap_or(cli.listen)));
    let mut config = Config::new(&public_url);
    config.require_sign_in = cli.require_sign_in;
    config.admins = cli.admins;
    tracing::info!("riff-server listens on {}", listener.local_addr()?);
    if let Some(client_id) = cli.client_id {
        tracing::info!("sign-in with {}", cli.issuer);
        config.provider = Some(Provider {
            issuer: cli.issuer,
            client_id,
            client_secret: cli.client_secret,
            allowed_domains: cli.allowed_domains,
        });
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
            // Save each unsaved change, then exit (R129).
            tracing::info!("stopping: saving the state");
            service.save().await.map_err(std::io::Error::other)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn each_setting_is_an_env_of_the_cli() {
        let cli = Cli::parse_from([
            "riff-server",
            "--admin=a",
            "--public-url=https://x",
            "--require-sign-in",
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
}
