//! doc-uni-proc - Main Entry Point
//!
//! Startup order (critical for Render deployments):
//!   1. Tracing + config
//!   2. ── Bind TCP port ──  ← Render sees service live immediately
//!   3. FalkorDB pool (lazy – smoke-test capped at 10 s total)
//!   4. Build processor + middleware
//!   5. Kafka consumer (background task)
//!   6. axum::serve

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
    // ── Environment variables ────────────────────────────────────────────────
    dotenvy::from_filename_override(".env.map").ok();
    dotenvy::from_filename_override(".env.secret").ok();
    dotenvy::from_filename_override(".env.local").ok();

    // ── Tracing ─────────────────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info,unified_processor_lib=debug,unified_processor=debug,tower_http=debug".into()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .json()
        )
        .init();

    // ── Config ───────────────────────────────────────────────────────────────
    let config = Config::from_env()?;
    let addr_str = format!("{}:{}", config.server.host, config.server.port);

    tracing::info!(
        addr = %addr_str,
        falkordb_host = %config.falkordb.host,
        falkordb_port = config.falkordb.port,
        falkordb_tls  = config.falkordb.use_tls,
        falkordb_user = %config.falkordb.username,
        "Starting doc-uni-proc"
    );

    // ── Step 1: Bind TCP port FIRST ──────────────────────────────────────────
    // Render kills a deploy if no port is detected within ~15 minutes.
    // Binding here — before FalkorDB — ensures Render sees us immediately.
    let listener = tokio::net::TcpListener::bind(&addr_str).await?;
    tracing::info!(bound_addr = %addr_str, "TCP listener bound — service accepting connections");

    // ── Step 2: FalkorDB connection pool ─────────────────────────────────────
    let falkordb_storage = create_falkordb_storage(
        &config.falkordb.host,
        config.falkordb.port,
        "default",
        &config.falkordb.username,
        config.falkordb.password.as_deref().unwrap_or(""),
        config.falkordb.use_tls,
        config.falkordb.embedding_dim,
    ).await?;
    tracing::info!("FalkorDB pool ready");

    // ── Step 3: Processor ────────────────────────────────────────────────────
    let processor = Arc::new(UnifiedProcessor::new(
        config.clone(),
        falkordb_storage.clone(),
    ).await?);

    // ── Step 4: Kafka consumer (background) ──────────────────────────────────
    let consumer_processor = processor.clone();
    tokio::spawn(async move {
        tracing::info!("Initializing Kafka event consumer...");
        let consumer = unified_processor_lib::graph::consumer::UnifiedEventConsumer::new(consumer_processor);
        if let Err(e) = consumer.start().await {
            tracing::error!("Kafka consumer failed to start: {}", e);
        }
    });

    // ── Step 5: Router + middleware ──────────────────────────────────────────
    let auth_layer = unified_processor_lib::infra::middleware::AxumAuthLayer::new(
        config.server.auth_middleware_url.clone(),
    );

    let rate_limit = unified_processor_lib::infra::middleware::AxumRateLimitConfig::default_for_service(10000);

    let app = build_app_router(processor.clone(), auth_layer, rate_limit);

    // ── Step 6: Serve ────────────────────────────────────────────────────────
    tracing::info!(addr = %addr_str, "Serving");
    axum::serve(listener, app).await?;

    Ok(())
}
