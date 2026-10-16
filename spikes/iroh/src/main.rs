//! The iroh spike of riff 3.0 (#697). See `README.md`.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    iroh_spike::main(std::env::args().skip(1).collect()).await
}
