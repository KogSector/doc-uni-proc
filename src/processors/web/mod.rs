//! Web processing module.
//!
//! Provides URL-based website scraping, HTML parsing, multi-page BFS
//! crawling, and URL specific logic.

pub mod types;
pub mod client;
pub mod scraper;
pub mod graph;

pub use types::*;
pub use client::*;
pub use scraper::*;
