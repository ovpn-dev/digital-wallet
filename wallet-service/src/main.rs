mod errors;
mod handlers;
mod kafka;
mod models;
mod repository;

use crate::handlers::AppState;
use crate::kafka::KafkaProducer;
use crate::repository::WalletRepository;
use axum::{
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (structured logging)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wallet_service=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Get configuration from environment
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/wallet_db".to_string());

    let kafka_brokers = std::env::var("KAFKA_BROKERS")
        .unwrap_or_else(|_| "localhost:9092".to_string());

    let kafka_topic = std::env::var("KAFKA_TOPIC")
        .unwrap_or_else(|_| "wallet-events".to_string());

    let server_port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    tracing::info!("Starting Wallet Service");
    tracing::info!("Database: {}", database_url);
    tracing::info!("Kafka brokers: {}", kafka_brokers);
    tracing::info!("Kafka topic: {}", kafka_topic);

    // Set up database connection pool
    // Why pool?
    // - Reuse connections (expensive to create)
    // - Limit concurrent connections to database
    // - Automatic connection management
    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Migrations completed successfully");

    // Create repository
    let repository = WalletRepository::new(pool);

    // Create Kafka producer
    tracing::info!("Initializing Kafka producer...");
    let kafka_producer = Arc::new(KafkaProducer::new(&kafka_brokers, kafka_topic)?);
    tracing::info!("Kafka producer initialized");

    // Create application state
    let state = AppState {
        repository,
        kafka_producer,
    };

    // Build the router with all routes
    let app = Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // Wallet management
        .route("/wallets", post(handlers::create_wallet))
        .route("/wallets/:wallet_id", get(handlers::get_wallet))
        .route("/users/:user_id/wallets", get(handlers::get_user_wallets))
        // Wallet operations
        .route("/wallets/:wallet_id/fund", post(handlers::fund_wallet))
        .route("/wallets/:wallet_id/transfer", post(handlers::transfer))
        // Add state and middleware
        .with_state(state)
        .layer(TraceLayer::new_for_http()); // Request/response logging

    // Start the server
    let addr = format!("0.0.0.0:{}", server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🚀 Wallet Service listening on {}", addr);
    tracing::info!("📝 API Documentation:");
    tracing::info!("  POST   /wallets                    - Create wallet");
    tracing::info!("  GET    /wallets/:wallet_id         - Get wallet");
    tracing::info!("  GET    /users/:user_id/wallets     - Get user's wallets");
    tracing::info!("  POST   /wallets/:wallet_id/fund    - Fund wallet");
    tracing::info!("  POST   /wallets/:wallet_id/transfer - Transfer money");
    tracing::info!("  GET    /health                      - Health check");

    axum::serve(listener, app).await?;

    Ok(())
}
