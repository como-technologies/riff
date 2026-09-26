//! The central service that sessions connect to.

use std::net::SocketAddr;

use clap::Parser;

/// The central service that sessions connect to.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The address to listen on.
    #[arg(long, env = "RIFF_LISTEN", default_value = "127.0.0.1:7878")]
    listen: SocketAddr,
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
    tracing::info!("riff-server listens on {}", listener.local_addr()?);
    axum::serve(listener, riff_server::router()).await
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
