//! Unified Processor Service - Main Entry Point
//!
//! Axum web server providing REST API for document and code processing.
//! Kafka-based pipeline: chunk → embeddings-service → FalkorDB (via Redis/6379)

use std::net::SocketAddr;
use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use unified_processor_lib::{
    core::routes::build_app_router,
    core::Config,
    core::orchestrator::UnifiedProcessor,
    infra::storage::create_falkordb_storage,
    infra::events::check_kafka_health,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env
    dotenvy::from_filename_override(".env.map").ok();
    dotenvy::from_filename_override(".env.secret").ok();
    dotenvy::from_filename_override(".env.local").ok();
    // Initialize tracing
    let file_appender = tracing_appender::rolling::daily("logs", "unified-processor.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "unified_processor=debug,tower_http=debug".into()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .pretty()
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .json()
        )
        .init();

    // Load configuration
    let config = Config::from_env()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));

    tracing::info!(
        "Starting unified-processor on {}",
        addr
    );
// Check Kafka health before starting (Kafka is required)
    check_kafka_health().await?;
    tracing::info!("Kafka on Aiven is initialized");

    // 
    // Initialize FalkorDB storage (Redis protocol, port 6379)
    let falkordb_storage = create_falkordb_storage(
        &config.falkordb.host,
        config.falkordb.port,
        "default",
        &config.falkordb.username,
        config.falkordb.password.as_deref().unwrap_or(""),
        config.falkordb.use_tls,
        config.falkordb.embedding_dim,
    ).await?;

    let processor = Arc::new(UnifiedProcessor::new(
        config.clone(),
        falkordb_storage.clone(),
    ).await?);

    #[cfg(feature = "kafka")]
    {
        let consumer_processor = processor.clone();
        tokio::spawn(async move {
            tracing::info!("Initializing Kafka event consumer...");
            let consumer = unified_processor_lib::graph::consumer::UnifiedEventConsumer::new(consumer_processor);
            if let Err(e) = consumer.start().await {
                tracing::error!("Kafka consumer failed to start: {}", e);
            }
        });
    }

    let auth_layer = unified_processor_lib::infra::middleware::AxumAuthLayer::with_grpc(
        config.server.auth_middleware_url.clone(),
        config.server.auth_grpc_url.clone(),
    ).await;

    let rate_limit = unified_processor_lib::infra::middleware::AxumRateLimitConfig::default_for_service(10000);

    let app = build_app_router(processor.clone(), auth_layer, rate_limit);

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    tracing::info!("Unified processor listening on {}", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
