//! Event publishing and consuming for unified-processor

pub mod producer;
pub mod topics;
pub mod types;

pub use producer::{EventProducer, ChunkEventPublisher};
pub use types::*;

/// Check Kafka health on startup with a resilient retry loop
pub async fn check_kafka_health() -> anyhow::Result<()> {
    let mut attempts = 0;
    let max_attempts = 60; // 60 * 5s = 5 minutes timeout

    while attempts < max_attempts {
        if let Ok(bootstrap_servers) = std::env::var("KAFKA_BOOTSTRAP_SERVERS") {
            if EventProducer::new(&bootstrap_servers).is_ok() {
                tracing::info!("Kafka health check passed");
                return Ok(());
            }
        }
        
        attempts += 1;
        tracing::warn!(
            "Kafka health check failed. Retrying in 5 seconds... (Attempt {}/{})",
            attempts, max_attempts
        );
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    anyhow::bail!("Kafka health check failed after {} attempts - service cannot start without Kafka", max_attempts);
}
