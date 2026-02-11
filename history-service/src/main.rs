mod consumer;
mod errors;
mod handlers;
mod models;
mod repository;

use crate::consumer::EventConsumer;
use crate::handlers::AppState;
use crate::repository::EventRepository;
use axum::{
    routing::get,
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "history_service=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Get configuration
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wallet_user:wallet_pass@localhost:5432/wallet_db".to_string());

    let kafka_brokers = std::env::var("KAFKA_BROKERS")
        .unwrap_or_else(|_| "localhost:9092".to_string());

    let kafka_topic = std::env::var("KAFKA_TOPIC")
        .unwrap_or_else(|_| "wallet-events".to_string());

    let kafka_group_id = std::env::var("KAFKA_GROUP_ID")
        .unwrap_or_else(|_| "history-service-group".to_string());

    let server_port = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>()?;

    tracing::info!("Starting History Service");
    tracing::info!("Database: {}", database_url);
    tracing::info!("Kafka brokers: {}", kafka_brokers);
    tracing::info!("Kafka topic: {}", kafka_topic);
    tracing::info!("Consumer group: {}", kafka_group_id);

    // Set up database connection pool
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
    let repository = EventRepository::new(pool.clone());

    // Create Kafka consumer
    tracing::info!("Initializing Kafka consumer...");
    let consumer = EventConsumer::new(
        &kafka_brokers,
        &kafka_group_id,
        &kafka_topic,
        repository.clone(),
    )?;
    tracing::info!("Kafka consumer initialized");

    // Spawn Kafka consumer in background task
    // This runs forever, processing events as they arrive
    tokio::spawn(async move {
        if let Err(e) = consumer.start().await {
            tracing::error!(error = %e, "Kafka consumer failed");
        }
    });

    // Create application state
    let state = AppState { repository };

    // Build the router with all routes
    let app = Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // History endpoints
        .route("/wallets/:wallet_id/history", get(handlers::get_wallet_history))
        .route("/users/:user_id/activity", get(handlers::get_user_activity))
        // Add state and middleware
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // Start the HTTP server
    let addr = format!("0.0.0.0:{}", server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🚀 History Service listening on {}", addr);
    tracing::info!("📝 API Documentation:");
    tracing::info!("  GET    /wallets/:wallet_id/history - Get wallet transaction history");
    tracing::info!("  GET    /users/:user_id/activity    - Get user activity");
    tracing::info!("  GET    /health                      - Health check");
    tracing::info!("🎧 Kafka consumer running in background...");

    axum::serve(listener, app).await?;

    Ok(())
}
