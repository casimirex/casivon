use casivon_backend::app::create_app;
use casivon_backend::config::AppConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "casivon_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env()?;
    let app = create_app(config.clone()).await?;

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", config.host, config.port))
        .await?;

    tracing::info!("Server running on http://{}:{}", config.host, config.port);
    axum::serve(listener, app).await?;

    Ok(())
}
