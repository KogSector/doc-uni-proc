// --- Start of error.rs ---
//! Error types for URL processing

use thiserror::Error;

/// Main error type for URL processing operations
#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Parsing error: {0}")]
    ParsingError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),

    #[error("External API error: {0}")]
    ExternalApiError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Circuit breaker open")]
    CircuitBreakerOpen,

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Rate limit exceeded: {retry_after:?}")]
    RateLimitExceeded { retry_after: Option<std::time::Duration> },

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

/// Error type categorization for monitoring and handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    NetworkError,
    ParsingError,
    DatabaseError,
    ResourceExhaustion,
    ExternalApiError,
    ValidationError,
    ConfigurationError,
}

impl ProcessingError {
    /// Get the error type category
    pub fn error_type(&self) -> ErrorType {
        match self {
            Self::NetworkError(_) | Self::HttpError(_) | Self::TimeoutError(_) => {
                ErrorType::NetworkError
            }
            Self::ParsingError(_) => ErrorType::ParsingError,
            Self::DatabaseError(_) => ErrorType::DatabaseError,
            Self::ResourceExhaustion(_) => ErrorType::ResourceExhaustion,
            Self::ExternalApiError(_) | Self::RateLimitExceeded { .. } => {
                ErrorType::ExternalApiError
            }
            Self::ValidationError(_) | Self::NotFound(_) => ErrorType::ValidationError,
            Self::ConfigurationError(_) | Self::AuthenticationError(_) => {
                ErrorType::ConfigurationError
            }
            Self::CircuitBreakerOpen => ErrorType::NetworkError,
            Self::IoError(_) | Self::JsonError(_) => ErrorType::ValidationError,
        }
    }

    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkError(_)
                | Self::TimeoutError(_)
                | Self::HttpError(_)
                | Self::DatabaseError(_)
        )
    }
}

pub type Result<T> = std::result::Result<T, ProcessingError>;


// --- Start of scraper_types.rs ---
/// Web scraping data structures for the unified processor.
///
/// These types represent the output of web scraping and crawling operations,
/// designed for downstream chunking, embedding, and FalkorDB storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Single page data ──────────────────────────────────────────────────────────

/// Complete representation of a scraped web page.
///
/// This is the primary output of `WebScraper::scrape_url()` and is designed
/// to be chunked, embedded, and stored in FalkorDB like code and document data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPageData {
    /// Canonical URL of the page (after redirect resolution).
    pub url: String,
    /// Domain extracted from the URL (e.g. `docs.example.com`).
    pub domain: String,
    /// Page `<title>` tag content.
    pub title: String,
    /// Meta description content.
    pub description: Option<String>,
    /// Cleaned main text content (HTML tags stripped, whitespace normalised).
    pub main_content: String,
    /// Raw HTML source.
    pub raw_html: String,
    /// Extracted CSS resources (inline `<style>` + linked stylesheets).
    pub css_resources: Vec<CssResource>,
    /// Extracted JS resources (inline `<script>` + external).
    pub js_resources: Vec<JsResource>,
    /// Heading hierarchy (`h1`–`h6`).
    pub headings: Vec<Heading>,
    /// All discovered links (internal + external).
    pub links: Vec<PageLink>,
    /// Image metadata.
    pub images: Vec<ImageInfo>,
    /// Table data extracted from `<table>` elements.
    pub tables: Vec<TableData>,
    /// Open Graph, Twitter Card, and other metadata.
    pub metadata: PageMetadata,
    /// JSON-LD structured data found on the page.
    pub structured_data: Vec<serde_json::Value>,
    /// Word count of main content.
    pub word_count: usize,
    /// HTTP status code of the response.
    pub status_code: u16,
    /// HTTP Content-Type header.
    pub content_type: String,
    /// Total response size in bytes.
    pub response_size: u64,
    /// Timestamp of fetch.
    pub fetched_at: DateTime<Utc>,
}

// ─── Website-level context (multi-page crawl) ───────────────────────────────

/// Result of a multi-page crawl.
///
/// Returned by `WebScraper::crawl_website()`. Contains all pages discovered
/// during BFS traversal plus aggregate statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSiteContext {
    /// The URL that the crawl started from.
    pub seed_url: String,
    /// Domain of the seed URL.
    pub domain: String,
    /// All successfully scraped pages.
    pub pages: Vec<WebPageData>,
    /// Sitemap of all discovered URLs (including ones not fetched due to limits).
    pub sitemap: Vec<SitemapEntry>,
    /// Total pages successfully scraped.
    pub total_pages: usize,
    /// Total bytes downloaded across all pages.
    pub total_size: u64,
    /// Time taken for the entire crawl in milliseconds.
    pub crawl_duration_ms: u64,
    /// Unique request ID for tracking.
    pub request_id: String,
}

// ─── Crawl configuration ───────────────────────────────────────────────────────

/// Configures the BFS crawl behaviour.
///
/// All fields have sensible defaults for safe, responsible crawling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlConfig {
    /// Maximum number of pages to fetch (back-pressure, prevents runaway crawls).
    pub max_pages: usize,
    /// Maximum link depth from the seed URL.
    pub max_depth: usize,
    /// Milliseconds to wait between consecutive requests to the same host.
    pub crawl_delay_ms: u64,
    /// Only follow links on the same domain as the seed URL.
    pub same_domain_only: bool,
    /// Capture inline/linked CSS content.
    pub include_css: bool,
    /// Capture inline/external JS content.
    pub include_js: bool,
    /// Download image bytes (false = metadata only).
    pub include_images: bool,
    /// User-Agent header sent with every request.
    pub user_agent: String,
    /// Per-request HTTP timeout in seconds.
    pub request_timeout_secs: u64,
    /// URL patterns to exclude (glob-style, e.g. `*/login*`)
    pub exclude_patterns: Vec<String>,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_pages: 50,
            max_depth: 5,
            crawl_delay_ms: 500,
            same_domain_only: true,
            include_css: true,
            include_js: true,
            include_images: false,
            user_agent: "ConFuseBot/1.0 (+https://confuse.ai/bot)".to_string(),
            request_timeout_secs: 30,
            exclude_patterns: vec![
                "*/login*".to_string(),
                "*/logout*".to_string(),
                "*/admin*".to_string(),
                "*.pdf".to_string(),
                "*.zip".to_string(),
            ],
        }
    }
}

// ─── Sub-structures ────────────────────────────────────────────────────────────

/// CSS resource (inline or linked).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CssResource {
    /// `Some(url)` for linked stylesheets, `None` for inline `<style>`.
    pub source_url: Option<String>,
    /// CSS content text.
    pub content: String,
    /// Whether this is inline (`<style>`) or external (`<link>`).
    pub is_inline: bool,
}

/// JavaScript resource (inline or external).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsResource {
    /// `Some(url)` for external scripts, `None` for inline `<script>`.
    pub source_url: Option<String>,
    /// JS content text.
    pub content: String,
    /// Whether this is inline.
    pub is_inline: bool,
}

/// Heading extracted from `h1`–`h6` tags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    /// Heading level (1–6).
    pub level: u8,
    /// Heading text content.
    pub text: String,
    /// Optional `id` attribute (for anchor links).
    pub id: Option<String>,
}

/// A hyperlink found on the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    /// Resolved absolute URL.
    pub url: String,
    /// Anchor text.
    pub text: String,
    /// Whether this link is on the same domain.
    pub is_internal: bool,
    /// `rel` attribute value (e.g. `nofollow`).
    pub rel: Option<String>,
}

/// Image metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Resolved absolute URL.
    pub url: String,
    /// Alt text.
    pub alt: String,
    /// Width attribute (if present).
    pub width: Option<u32>,
    /// Height attribute (if present).
    pub height: Option<u32>,
}

/// Table data extracted from `<table>` elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableData {
    /// Table index on the page (0-based).
    pub index: usize,
    /// Optional `<caption>` text.
    pub caption: Option<String>,
    /// Header row cells.
    pub headers: Vec<String>,
    /// Data rows (each row is a Vec of cell texts).
    pub rows: Vec<Vec<String>>,
}

/// Comprehensive page metadata (Open Graph, Twitter Cards, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageMetadata {
    /// Canonical URL from `<link rel="canonical">`.
    pub canonical_url: Option<String>,
    /// Language from `<html lang="...">`.
    pub language: Option<String>,
    /// Charset from `<meta charset="...">`.
    pub charset: Option<String>,
    /// Author from `<meta name="author">`.
    pub author: Option<String>,
    /// Keywords from `<meta name="keywords">`.
    pub keywords: Vec<String>,
    /// Open Graph properties (`og:title`, `og:image`, etc.).
    pub og_properties: HashMap<String, String>,
    /// Twitter Card properties.
    pub twitter_properties: HashMap<String, String>,
    /// Favicon URL.
    pub favicon_url: Option<String>,
    /// robots meta content.
    pub robots: Option<String>,
}

/// Entry in the sitemap built during crawling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitemapEntry {
    /// Absolute URL.
    pub url: String,
    /// Depth from the seed URL (0 = seed).
    pub depth: usize,
    /// Whether this URL was successfully fetched.
    pub fetched: bool,
    /// Optional title (populated after fetch).
    pub title: Option<String>,
}

/// Crawl progress — used for status polling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlProgress {
    pub request_id: String,
    pub seed_url: String,
    pub pages_fetched: usize,
    pub pages_queued: usize,
    pub total_size_bytes: u64,
    pub elapsed_ms: u64,
    pub status: CrawlStatus,
}

/// Crawl lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrawlStatus {
    Queued,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

// --- Start of url_types.rs ---
/// Core types for URL processing


use std::time::Duration;
use uuid::Uuid;

// ============================================================================
// Job Management Types
// ============================================================================

/// URL processing job tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlProcessingJob {
    pub job_id: Uuid,
    pub url: String,
    pub job_type: JobType,
    pub status: JobStatus,
    pub config: ProcessingConfig,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub stats: ProcessingStats,
}

/// Type of URL processing job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobType {
    /// Process a single URL
    SingleUrl,
    /// Crawl URLs with depth
    Crawl,
    /// Extract repository metadata
    Repository,
}

/// Status of a URL processing job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Configuration for URL processing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Maximum crawl depth (0-10)
    pub crawl_depth: u32,
    /// Only crawl URLs within the same domain
    pub same_domain_only: bool,
    /// Enable JavaScript rendering for dynamic content
    pub enable_js_rendering: bool,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Chunk size range (min, max) in characters
    pub chunk_size_range: (usize, usize),
    /// Custom HTTP headers
    pub custom_headers: HashMap<String, String>,
    /// User agent string
    pub user_agent: String,
    /// URL filtering patterns (regex)
    pub url_filter_patterns: Vec<String>,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            crawl_depth: 0,
            same_domain_only: true,
            enable_js_rendering: false,
            timeout_seconds: 30,
            max_concurrent_requests: 10,
            chunk_size_range: (500, 2000),
            custom_headers: HashMap::new(),
            user_agent: "ConFuseBot/1.0 (+https://confuse.ai/bot)".to_string(),
            url_filter_patterns: Vec::new(),
        }
    }
}

/// Statistics for URL processing operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessingStats {
    pub urls_processed: u32,
    pub urls_failed: u32,
    pub chunks_created: u32,
    pub edges_created: u32,
    pub total_content_size: u64,
    pub processing_duration_ms: u64,
}

// ============================================================================
// Crawler Types
// ============================================================================

/// A task for the web crawler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlTask {
    pub url: String,
    pub depth: u32,
    pub parent_url: Option<String>,
}

/// Configuration for the web crawler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    /// Maximum crawl depth
    pub max_depth: u32,
    /// Only crawl URLs within the same domain
    pub same_domain_only: bool,
    /// Maximum concurrent requests per domain
    pub max_concurrent_requests: usize,
    /// Rate limit delay per domain
    pub rate_limit_per_domain: Duration,
    /// Respect robots.txt directives
    pub respect_robots_txt: bool,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            same_domain_only: true,
            max_concurrent_requests: 10,
            rate_limit_per_domain: Duration::from_millis(500),
            respect_robots_txt: true,
        }
    }
}

/// Result of crawling a web page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    pub url: String,
    pub depth: u32,
    pub parent_url: Option<String>,
    pub http_status: u16,
    pub content_type: String,
    pub html_content: String,
    pub extracted_content: ExtractedContent,
    pub outbound_links: Vec<String>,
    pub crawl_timestamp: DateTime<Utc>,
    pub response_time_ms: u64,
}

// ============================================================================
// Content Extraction Types
// ============================================================================

/// Extracted content from a web page
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedContent {
    pub main_content: String,
    pub code_blocks: Vec<CodeBlock>,
    pub tables: Vec<Table>,
    pub metadata: ContentMetadata,
    pub structured_data: Option<StructuredData>,
}

/// A code block extracted from HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub content: String,
    pub language: Option<String>,
    pub line_start: Option<usize>,
}

/// A table extracted from HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub caption: Option<String>,
}

/// Metadata extracted from a web page
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub publication_date: Option<DateTime<Utc>>,
    pub language: Option<String>,
    pub image_alt_texts: Vec<String>,
}

/// Structured data extracted from HTML (JSON-LD, microdata, RDFa)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredData {
    pub schema_type: String,
    pub data: serde_json::Value,
}

// ============================================================================
// Repository Metadata Types
// ============================================================================

/// Repository metadata from code hosting platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    pub platform: RepositoryPlatform,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub primary_language: Option<String>,
    pub star_count: u32,
    pub fork_count: u32,
    pub last_update: DateTime<Utc>,
    pub readme_content: Option<String>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub contributor_count: u32,
    pub top_contributors: Vec<Contributor>,
}

/// Code hosting platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryPlatform {
    GitHub,
    GitLab,
    Bitbucket,
}

/// Repository contributor information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contributor {
    pub username: String,
    pub contributions: u32,
}

// ============================================================================
// Chunking Types
// ============================================================================

/// Configuration for content chunking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkerConfig {
    pub min_chunk_size: usize,
    pub max_chunk_size: usize,
    pub preserve_paragraphs: bool,
    pub preserve_code_blocks: bool,
    pub preserve_tables: bool,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            min_chunk_size: 500,
            max_chunk_size: 2000,
            preserve_paragraphs: true,
            preserve_code_blocks: true,
            preserve_tables: true,
        }
    }
}

/// A chunk of URL content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlChunk {
    pub chunk_id: String,
    pub content: String,
    pub source_url: String,
    pub chunk_position: u32,
    pub content_type: ContentType,
    pub context_metadata: ChunkContext,
    pub extraction_timestamp: DateTime<Utc>,
}

/// Type of content in a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    Text,
    Code,
    Table,
    List,
}

/// Context metadata for a chunk
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkContext {
    pub heading_hierarchy: Vec<String>,
    pub semantic_tags: Vec<String>,
    pub position_in_document: f32,
}

// ============================================================================
// Knowledge Graph Types
// ============================================================================

/// Configuration for knowledge graph building
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphBuilderConfig {
    pub batch_size: usize,
    pub similarity_threshold: f32,
    pub max_edge_types: usize,
}

impl Default for GraphBuilderConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            similarity_threshold: 0.8,
            max_edge_types: 1000,
        }
    }
}

/// A page node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageNode {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub processing_timestamp: DateTime<Utc>,
}

/// A chunk node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkNode {
    pub chunk_id: String,
    pub content: String,
    pub source_url: String,
    pub chunk_position: u32,
    pub content_type: String,
    pub extraction_timestamp: DateTime<Utc>,
    pub embedding: Option<Vec<f32>>,
}

/// A repository node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryNode {
    pub platform: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub primary_language: Option<String>,
    pub star_count: u32,
    pub fork_count: u32,
}

// ============================================================================
// Relationship Types
// ============================================================================

/// An extracted relationship between content chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source_chunk_id: String,
    pub target_chunk_id: String,
    pub relationship_type: EdgeType,
    pub confidence: f32,
    pub metadata: RelationshipMetadata,
}

/// Metadata for a relationship
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationshipMetadata {
    pub weight: f32,
    pub created_at: DateTime<Utc>,
    pub additional_properties: HashMap<String, String>,
}

/// Edge types in the knowledge graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    // Structural edges
    HasChunk,
    NextChunk,
    LinksTo,
    ContainsCode,
    PartOf,

    // Semantic edges
    SimilarTo,
    References,
    Defines,
    Explains,
    Contradicts,
    MentionsSameEntity,

    // Temporal edges
    Precedes,
    Follows,
    UpdatedFrom,
    Supersedes,

    // Metadata edges
    AuthoredBy,
    TaggedWith,
}

impl EdgeType {
    /// Get the category of this edge type
    pub fn category(&self) -> EdgeCategory {
        match self {
            Self::HasChunk
            | Self::NextChunk
            | Self::LinksTo
            | Self::ContainsCode
            | Self::PartOf => EdgeCategory::Structural,

            Self::SimilarTo
            | Self::References
            | Self::Defines
            | Self::Explains
            | Self::Contradicts
            | Self::MentionsSameEntity => EdgeCategory::Semantic,

            Self::Precedes | Self::Follows | Self::UpdatedFrom | Self::Supersedes => {
                EdgeCategory::Temporal
            }

            Self::AuthoredBy | Self::TaggedWith => EdgeCategory::Metadata,
        }
    }

    /// Get the string representation for Cypher queries
    pub fn as_cypher_label(&self) -> &'static str {
        match self {
            Self::HasChunk => "HAS_CHUNK",
            Self::NextChunk => "NEXT_CHUNK",
            Self::LinksTo => "LINKS_TO",
            Self::ContainsCode => "CONTAINS_CODE",
            Self::PartOf => "PART_OF",
            Self::SimilarTo => "SIMILAR_TO",
            Self::References => "REFERENCES",
            Self::Defines => "DEFINES",
            Self::Explains => "EXPLAINS",
            Self::Contradicts => "CONTRADICTS",
            Self::MentionsSameEntity => "MENTIONS_SAME_ENTITY",
            Self::Precedes => "PRECEDES",
            Self::Follows => "FOLLOWS",
            Self::UpdatedFrom => "UPDATED_FROM",
            Self::Supersedes => "SUPERSEDES",
            Self::AuthoredBy => "AUTHORED_BY",
            Self::TaggedWith => "TAGGED_WITH",
        }
    }
}

/// Edge category for taxonomy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeCategory {
    Structural,
    Semantic,
    Temporal,
    Metadata,
}

// ============================================================================
// Entity Recognition Types
// ============================================================================

/// An entity extracted from content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub text: String,
    pub entity_type: EntityType,
    pub start_offset: usize,
    pub end_offset: usize,
}

/// Type of entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Date,
    Url,
    CodeReference,
    TechnicalTerm,
}


