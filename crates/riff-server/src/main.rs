//! The central service that sessions connect to.

use std::net::SocketAddr;

use clap::Parser;
use riff_server::Service;
use riff_server::auth::Config;

/// The central service that sessions connect to.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The address to listen on.
    #[arg(long, env = "RIFF_LISTEN", default_value = "127.0.0.1:7878")]
    listen: SocketAddr,

    /// A person who may revoke the tokens of any person. Repeat it for
    /// more admins.
    #[arg(long = "admin", env = "RIFF_ADMINS", value_delimiter = ',')]
    admins: Vec<String>,

    /// The URL where people reach the server. It is the OAuth resource
    /// and issuer. The default is http://<listen>.
    #[arg(long, env = "RIFF_PUBLIC_URL")]
    public_url: Option<String>,

    /// Refuse each request that has no live riff access token.
    #[arg(long, env = "RIFF_REQUIRE_SIGN_IN")]
    require_sign_in: bool,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    let public_url = cli
        .public_url
        .unwrap_or_else(|| format!("http://{}", listener.local_addr().unwrap_or(cli.listen)));
    let mut config = Config::new(&public_url);
    config.require_sign_in = cli.require_sign_in;
    config.admins = cli.admins;
    tracing::info!("riff-server listens on {}", listener.local_addr()?);
    axum::serve(listener, Service::new(config).router()).await
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
