//! The central service that sessions connect to.

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use riff_server::auth::Config;
use riff_server::gcs::Gcs;
use riff_server::oidc::{self, DEFAULT_DOMAIN, Provider, SignInError};
use riff_server::{Service, listen};
use tokio::signal::unix::{SignalKind, signal};

/// The central service that sessions connect to. It runs in the
/// foreground (R118).
#[derive(Parser)]
#[command(version = riff_core::build::VERSION, about, max_term_width = 80)]
struct Cli {
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

    /// The Cloud Storage bucket that holds the state. The server loads
    /// the state at start and saves each change. Without it, the server
    /// saves nothing.
    #[arg(long, env = "RIFF_BUCKET", hide_env_values = true)]
    bucket: Option<String>,

    /// The minutes that the owner has to answer `riff owner --take` of an
    /// admin. With no answer, the admin is the owner.
    #[arg(long, env = "RIFF_OWNER_TAKE_MINUTES", hide_env_values = true, default_value_t = 10,
          value_parser = clap::value_parser!(u64).range(1..))]
    owner_take_minutes: u64,

    /// The minutes between two checks of the owner. A check misses when
    /// the owner has no live lead session.
    #[arg(long, env = "RIFF_OWNER_PING_MINUTES", hide_env_values = true, default_value_t = 5,
          value_parser = clap::value_parser!(u64).range(1..))]
    owner_ping_minutes: u64,

    /// The misses in a row after which the owner is gone. The riff then
    /// has no owner, and asks each admin for a volunteer.
    #[arg(long, env = "RIFF_OWNER_PINGS", hide_env_values = true, default_value_t = 3,
          value_parser = clap::value_parser!(u32).range(1..))]
    owner_pings: u32,
}

impl Cli {
    /// True for a riff with no sign-in (R211).
    fn trusted(&self) -> bool {
        self.client_id.is_none() && !self.require_sign_in
    }
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
    let cli = Cli::parse();
    if let Some(owner) = cli.owner.as_deref().filter(|o| !o.contains('@')) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("the owner {owner} is not an email"),
        ));
    }
    // The bucket can hold an owner: check again after the load.
    let trusted = cli.trusted();
    let owned = cli.owner.is_some() || cli.bucket.is_some();
    let warning = listen::check(cli.listen, trusted, cli.insecure, owned)
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
            match Service::load(config, store).await {
                Ok(service) => service,
                Err(e) => {
                    // The text names the object and the fix
                    // (01M3MMXYS1V8CA89D2XHKPR6C4).
                    tracing::error!("riff-server stops: {e}");
                    std::process::exit(1);
                }
            }
        }
        None => {
            tracing::warn!("no RIFF_BUCKET: the state is not saved");
            Service::new(config)
        }
    };
    let owned = service.tokens().owned();
    listen::check(cli.listen, trusted, cli.insecure, owned)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
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
    fn the_cli_has_no_subcommand() {
        assert_eq!(Cli::command().get_subcommands().count(), 0);
    }
}
