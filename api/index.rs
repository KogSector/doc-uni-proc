//! Unified Processor Service - Main Entry Point
//!
//! Axum web server providing REST API for document and code processing.
//! Kafka-based pipeline: chunk → embeddings-service → FalkorDB (via Redis/6379)

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use unified_processor_lib::{
    core::routes::build_app_router,
    core::Config,
    core::orchestrator::UnifiedProcessor,
    infra::storage::create_falkordb_storage,
};


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env
    dotenvy::from_filename_override(".env.map").ok();
    dotenvy::from_filename_override(".env.secret").ok();
    dotenvy::from_filename_override(".env.local").ok();
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info,unified_processor_lib=debug,unified_processor=debug,tower_http=debug".into()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .json()
        )
        .init();

    // Load configuration
    let config = Config::from_env()?;
    let addr_str = format!("{}:{}", config.server.host, config.server.port);

    tracing::info!(
        "Starting unified-processor on {}",
        addr_str
    );

    // (Listener will be bound after initialization to prevent premature traffic routing) 
    tracing::info!("Initializing FalkorDB storage at {}:{}", config.falkordb.host, config.falkordb.port);
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
    tracing::info!("Successfully initialized FalkorDB storage");

    let processor = Arc::new(UnifiedProcessor::new(
        config.clone(),
        falkordb_storage.clone(),
    ).await?);

    let consumer_processor = processor.clone();
    tokio::spawn(async move {
        tracing::info!("Initializing Kafka event consumer...");
        let consumer = unified_processor_lib::graph::consumer::UnifiedEventConsumer::new(consumer_processor);
        if let Err(e) = consumer.start().await {
            tracing::error!("Kafka consumer failed to start: {}", e);
        }
    });



    let auth_layer = unified_processor_lib::infra::middleware::AxumAuthLayer::new(
        config.server.auth_middleware_url.clone(),
    );

    let rate_limit = unified_processor_lib::infra::middleware::AxumRateLimitConfig::default_for_service(10000);

    let app = build_app_router(processor.clone(), auth_layer, rate_limit);

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&addr_str).await?;
    tracing::info!("Unified processor listening on {}", addr_str);
    axum::serve(listener, app).await?;

    Ok(())
}
