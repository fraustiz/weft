use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let relay_name = std::env::var("WEFT_RELAY_NAME").unwrap_or_else(|_| "localhost".to_owned());
    let bind = std::env::var("WEFT_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());

    let listener = TcpListener::bind(&bind).await?;
    tracing::info!(%bind, %relay_name, "weft relay listening");
    weft_relay::serve(listener, relay_name).await
}
