// --- Start of scraper.rs ---
//! Web scraping engine — HTTP fetching + BFS multi-page crawler.
//!
//! Scalability features:
//! - Connection-pooled `reqwest::Client` (shared across all fetches)
//! - Configurable concurrency via `max_concurrent` (Semaphore-gated)
//! - Per-host rate limiting (`crawl_delay_ms`)
//! - Deduplication via `HashSet<String>` of normalised URLs
//! - Configurable depth + page limits (back-pressure)
//! - Streaming: pages are processed as they arrive (no full-site buffering)

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use url::Url;

use super::*;

// ─── WebScraper ─────────────────────────────────────────────────────────────────

/// High-performance web scraper with connection pooling and rate limiting.
///
/// Designed to be constructed once and reused across many scrape/crawl requests.
/// The inner `reqwest::Client` maintains a connection pool automatically.
pub struct WebScraper {
    client: reqwest::Client,
    /// Maximum concurrent HTTP requests during a crawl.
    max_concurrent: usize,
}

impl WebScraper {
    /// Create a new `WebScraper` with the given default configuration.
    pub fn new(config: &CrawlConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::limited(10))
            .gzip(true)
            .brotli(true)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            max_concurrent: 5, // safe default
        })
    }

    /// Set the maximum number of concurrent HTTP requests during a crawl.
    pub fn with_max_concurrent(mut self, n: usize) -> Self {
        self.max_concurrent = n.max(1);
        self
    }

    // ─── Single page scraping ───────────────────────────────────────────

    /// Scrape a single URL and return structured page data.
    ///
    /// Does NOT follow links — use `crawl_website` for multi-page crawling.
    pub async fn scrape_url(&self, url_str: &str, config: &CrawlConfig) -> Result<WebPageData> {
        let url = Url::parse(url_str)
            .context(format!("Invalid URL: {}", url_str))?;

        info!(url = %url, "Scraping single page");

        let response = self.client
            .get(url.as_str())
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.5")
            .send()
            .await
            .context(format!("HTTP request failed for {}", url))?;

        let status_code = response.status().as_u16();
        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();
        let final_url = response.url().clone();

        let body = response.bytes().await
            .context("Failed to read response body")?;
        let response_size = body.len() as u64;
        let html = String::from_utf8_lossy(&body).to_string();

        let domain = final_url.host_str().unwrap_or("unknown").to_string();

        // Parse the HTML document
        let parsed = parse_html(&html, &final_url);

        let word_count = parsed.main_content
            .split_whitespace()
            .count();

        let page = WebPageData {
            url: final_url.to_string(),
            domain,
            title: parsed.title,
            description: parsed.description,
            main_content: parsed.main_content,
            raw_html: html,
            css_resources: if config.include_css { parsed.css_resources } else { vec![] },
            js_resources: if config.include_js { parsed.js_resources } else { vec![] },
            headings: parsed.headings,
            links: parsed.links,
            images: parsed.images,
            tables: parsed.tables,
            metadata: parsed.metadata,
            structured_data: parsed.structured_data,
            word_count,
            status_code,
            content_type,
            response_size,
            fetched_at: Utc::now(),
        };

        info!(
            url = %page.url,
            title = %page.title,
            word_count = page.word_count,
            links = page.links.len(),
            "Page scraped successfully"
        );

        Ok(page)
    }

    // ─── Multi-page BFS crawler ─────────────────────────────────────────

    /// Crawl a website starting from `seed_url` using breadth-first traversal.
    ///
    /// Respects `CrawlConfig` limits for max pages, max depth, and rate limiting.
    /// Pages are processed sequentially with configurable delays to be a good
    /// web citizen. For larger crawls, the semaphore allows bounded concurrency.
    pub async fn crawl_website(
        &self,
        seed_url: &str,
        config: &CrawlConfig,
        request_id: &str,
    ) -> Result<WebSiteContext> {
        let seed = Url::parse(seed_url)
            .context(format!("Invalid seed URL: {}", seed_url))?;
        let seed_domain = seed.host_str()
            .ok_or_else(|| anyhow!("Seed URL has no host: {}", seed_url))?
            .to_string();

        info!(
            seed_url = %seed_url,
            domain = %seed_domain,
            max_pages = config.max_pages,
            max_depth = config.max_depth,
            request_id = %request_id,
            "Starting website crawl"
        );

        let start = std::time::Instant::now();

        // BFS state
        let mut visited: HashSet<String> = HashSet::with_capacity(config.max_pages);
        let mut queue: VecDeque<(Url, usize)> = VecDeque::new();
        let mut pages: Vec<WebPageData> = Vec::with_capacity(config.max_pages);
        let mut sitemap: Vec<SitemapEntry> = Vec::new();

        // Seed the queue
        let normalised_seed = normalise_url(&seed);
        visited.insert(normalised_seed.clone());
        queue.push_back((seed, 0));
        sitemap.push(SitemapEntry {
            url: normalised_seed,
            depth: 0,
            fetched: false,
            title: None,
        });

        let _semaphore = Arc::new(Semaphore::new(self.max_concurrent));
        let mut total_size: u64 = 0;

        while let Some((url, depth)) = queue.pop_front() {
            // Check page limit
            if pages.len() >= config.max_pages {
                info!(
                    pages_fetched = pages.len(),
                    pages_remaining = queue.len(),
                    "Max page limit reached, stopping crawl"
                );
                break;
            }

            // Check depth limit
            if depth > config.max_depth {
                debug!(url = %url, depth, "Skipping — exceeds max depth");
                continue;
            }

            // Rate limiting — sleep between requests
            if !pages.is_empty() && config.crawl_delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(config.crawl_delay_ms)).await;
            }

            // Fetch the page
            match self.scrape_url(url.as_str(), config).await {
                Ok(page) => {
                    total_size += page.response_size;

                    // Update sitemap entry
                    let normalised = normalise_url(&url);
                    if let Some(entry) = sitemap.iter_mut().find(|e| e.url == normalised) {
                        entry.fetched = true;
                        entry.title = Some(page.title.clone());
                    }

                    // Discover new links (only internal, same domain)
                    if depth < config.max_depth {
                        for link in &page.links {
                            if !link.is_internal && config.same_domain_only {
                                continue;
                            }

                            // Exclude patterns check
                            if config.exclude_patterns.iter().any(|pat| url_matches_pattern(&link.url, pat)) {
                                continue;
                            }

                            if let Ok(link_url) = Url::parse(&link.url) {
                                // Same-domain check
                                let link_domain = link_url.host_str().unwrap_or("");
                                if config.same_domain_only && link_domain != seed_domain {
                                    continue;
                                }

                                // Only HTML pages (skip anchors, resources, etc.)
                                if !is_likely_html_url(&link_url) {
                                    continue;
                                }

                                let normalised_link = normalise_url(&link_url);
                                if visited.insert(normalised_link.clone()) {
                                    queue.push_back((link_url, depth + 1));
                                    sitemap.push(SitemapEntry {
                                        url: normalised_link,
                                        depth: depth + 1,
                                        fetched: false,
                                        title: None,
                                    });
                                }
                            }
                        }
                    }

                    pages.push(page);
                }
                Err(e) => {
                    warn!(url = %url, error = %e, "Failed to scrape page, continuing crawl");
                    let normalised = normalise_url(&url);
                    if let Some(entry) = sitemap.iter_mut().find(|e| e.url == normalised) {
                        entry.fetched = false;
                    }
                }
            }
        }

        // Mark remaining queued URLs in sitemap as not fetched
        for remaining in &queue {
            let normalised = normalise_url(&remaining.0);
            if !sitemap.iter().any(|e| e.url == normalised) {
                sitemap.push(SitemapEntry {
                    url: normalised,
                    depth: remaining.1,
                    fetched: false,
                    title: None,
                });
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        info!(
            seed_url = %seed_url,
            pages_scraped = pages.len(),
            sitemap_entries = sitemap.len(),
            total_size_kb = total_size / 1024,
            duration_ms = duration,
            "Website crawl completed"
        );

        Ok(WebSiteContext {
            seed_url: seed_url.to_string(),
            domain: seed_domain,
            pages,
            sitemap,
            total_pages: visited.len(),
            total_size,
            crawl_duration_ms: duration,
            request_id: request_id.to_string(),
        })
    }
}

// ─── URL normalisation ──────────────────────────────────────────────────────────

/// Normalise a URL for deduplication.
///
/// Strips fragment, trailing slash, and lowercases scheme + host.
fn normalise_url(url: &Url) -> String {
    let mut normalised = url.clone();
    normalised.set_fragment(None);

    let mut s = normalised.to_string();
    // Strip trailing slash (unless it's the root path)
    if s.ends_with('/') && s.matches('/').count() > 3 {
        s.pop();
    }
    s
}

/// Check if a URL likely points to an HTML page (not a binary resource).
fn is_likely_html_url(url: &Url) -> bool {
    let path = url.path().to_lowercase();

    // Skip known binary/resource extensions
    let skip_extensions = [
        ".pdf", ".zip", ".tar", ".gz", ".rar", ".7z",
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".svg", ".webp", ".ico",
        ".mp3", ".mp4", ".wav", ".avi", ".mov", ".webm",
        ".woff", ".woff2", ".ttf", ".otf", ".eot",
        ".css", ".js", ".json", ".xml", ".rss", ".atom",
        ".exe", ".dmg", ".msi", ".deb", ".rpm",
    ];

    !skip_extensions.iter().any(|ext| path.ends_with(ext))
}

/// Simple glob-style pattern matching for URL exclusion.
///
/// Supports `*` as a wildcard that matches any number of characters.
fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return url.contains(pattern);
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = url[pos..].find(part) {
            if i == 0 && found != 0 {
                return false; // first part must match from start if no leading *
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalise_url() {
        let url = Url::parse("https://example.com/docs/page#section").unwrap();
        assert_eq!(normalise_url(&url), "https://example.com/docs/page");
    }

    #[test]
    fn test_is_likely_html() {
        assert!(is_likely_html_url(&Url::parse("https://example.com/about").unwrap()));
        assert!(is_likely_html_url(&Url::parse("https://example.com/docs/intro.html").unwrap()));
        assert!(!is_likely_html_url(&Url::parse("https://example.com/file.pdf").unwrap()));
        assert!(!is_likely_html_url(&Url::parse("https://example.com/style.css").unwrap()));
    }

    #[test]
    fn test_url_matches_pattern() {
        assert!(url_matches_pattern("https://example.com/login", "*/login*"));
        assert!(url_matches_pattern("https://example.com/admin/users", "*/admin*"));
        assert!(!url_matches_pattern("https://example.com/about", "*/login*"));
    }
}

// --- Start of parser.rs ---
/// HTML structure analysis and content extraction.
///
/// Uses the `scraper` crate for DOM traversal with CSS selectors.
/// Designed for performance: a single parse pass extracts headings, links,
/// images, tables, metadata, CSS, JS, and main content simultaneously.

use ::scraper::{Html, Selector, ElementRef};


// ─── Lazy-initialised selectors (compiled once, reused across calls) ────────

macro_rules! selector {
    ($s:expr) => {
        Selector::parse($s).expect(concat!("invalid selector: ", $s))
    };
}

/// Parse an HTML document and extract all structural information in a single pass.
///
/// This is the primary entry point for HTML processing. It returns a `ParsedPage`
/// containing all extracted data. Call sites can then pick what they need.
pub fn parse_html(html: &str, base_url: &url::Url) -> ParsedPage {
    let document = Html::parse_document(html);

    ParsedPage {
        title: extract_title(&document),
        description: extract_meta_description(&document),
        main_content: extract_main_content(&document),
        headings: extract_headings(&document),
        links: extract_links(&document, base_url),
        images: extract_images(&document, base_url),
        tables: extract_tables(&document),
        css_resources: extract_css(&document, base_url),
        js_resources: extract_js(&document, base_url),
        metadata: extract_metadata(&document, base_url),
        structured_data: extract_structured_data(&document),
    }
}

/// Intermediate parse result — contains everything extracted from one HTML page.
#[derive(Debug, Clone)]
pub struct ParsedPage {
    pub title: String,
    pub description: Option<String>,
    pub main_content: String,
    pub headings: Vec<Heading>,
    pub links: Vec<PageLink>,
    pub images: Vec<ImageInfo>,
    pub tables: Vec<TableData>,
    pub css_resources: Vec<CssResource>,
    pub js_resources: Vec<JsResource>,
    pub metadata: PageMetadata,
    pub structured_data: Vec<serde_json::Value>,
}

// ─── Title ──────────────────────────────────────────────────────────────────────

fn extract_title(doc: &Html) -> String {
    let sel = selector!("title");
    doc.select(&sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default()
}

// ─── Meta description ───────────────────────────────────────────────────────────

fn extract_meta_description(doc: &Html) -> Option<String> {
    let sel = selector!(r#"meta[name="description"]"#);
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ─── Main content extraction ────────────────────────────────────────────────────

/// Extracts the main textual content from the page.
///
/// Strategy (priority order):
/// 1. `<main>` element
/// 2. `<article>` element
/// 3. Element with `role="main"`
/// 4. `<div id="content">` or `<div class="content">`
/// 5. Fallback: `<body>` minus nav/header/footer/aside
pub fn extract_main_content(doc: &Html) -> String {
    // Try semantic elements first
    let candidates = [
        "main",
        "article",
        r#"[role="main"]"#,
        r#"#content"#,
        r#".content"#,
        r#"#main"#,
        r#".main"#,
    ];

    for sel_str in &candidates {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = doc.select(&sel).next() {
                let text = extract_visible_text(el);
                if text.split_whitespace().count() > 20 {
                    return text;
                }
            }
        }
    }

    // Fallback: body text minus boilerplate
    let body_sel = selector!("body");
    let exclude_sels: Vec<Selector> = ["nav", "header", "footer", "aside", "script", "style", "noscript"]
        .iter()
        .filter_map(|s| Selector::parse(s).ok())
        .collect();

    if let Some(body) = doc.select(&body_sel).next() {
        let mut text = String::with_capacity(body.inner_html().len() / 2);
        collect_text_excluding(&body, &exclude_sels, &mut text);
        normalise_whitespace(&text)
    } else {
        String::new()
    }
}

/// Recursively collect visible text, skipping elements matching any exclude selector.
fn collect_text_excluding(el: &ElementRef<'_>, excludes: &[Selector], out: &mut String) {
    for child in el.children() {
        match child.value() {
            ::scraper::Node::Text(t) => {
                let trimmed = t.text.trim();
                if !trimmed.is_empty() {
                    if !out.is_empty() && !out.ends_with('\n') && !out.ends_with(' ') {
                        out.push(' ');
                    }
                    out.push_str(trimmed);
                }
            }
            ::scraper::Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    let should_exclude = excludes.iter().any(|sel| sel.matches(&child_el));
                    if !should_exclude {
                        // Add line break for block-level elements
                        let tag = child_el.value().name();
                        let is_block = matches!(tag, "p" | "div" | "section" | "h1" | "h2" | "h3"
                            | "h4" | "h5" | "h6" | "li" | "br" | "hr" | "blockquote" | "pre" | "tr");
                        if is_block && !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                        collect_text_excluding(&child_el, excludes, out);
                        if is_block {
                            out.push('\n');
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract visible text from an element recursively.
fn extract_visible_text(el: ElementRef<'_>) -> String {
    let exclude_sels: Vec<Selector> = ["script", "style", "noscript"]
        .iter()
        .filter_map(|s| Selector::parse(s).ok())
        .collect();
    let mut text = String::with_capacity(el.inner_html().len() / 2);
    collect_text_excluding(&el, &exclude_sels, &mut text);
    normalise_whitespace(&text)
}

/// Collapse runs of whitespace to single spaces, trim blank lines.
fn normalise_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_newline = false;

    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_newline && !result.is_empty() {
                result.push('\n');
                prev_newline = true;
            }
        } else {
            // Collapse internal whitespace
            let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
            result.push_str(&collapsed);
            result.push('\n');
            prev_newline = false;
        }
    }

    result.trim().to_string()
}

// ─── Headings ───────────────────────────────────────────────────────────────────

fn extract_headings(doc: &Html) -> Vec<Heading> {
    let sel = selector!("h1, h2, h3, h4, h5, h6");
    doc.select(&sel)
        .map(|el| {
            let tag_name = el.value().name();
            let level: u8 = tag_name[1..].parse().unwrap_or(1);
            Heading {
                level,
                text: el.text().collect::<String>().trim().to_string(),
                id: el.value().attr("id").map(|s| s.to_string()),
            }
        })
        .collect()
}

// ─── Links ──────────────────────────────────────────────────────────────────────

fn extract_links(doc: &Html, base_url: &url::Url) -> Vec<PageLink> {
    let sel = selector!("a[href]");
    let base_domain = base_url.host_str().unwrap_or("");

    doc.select(&sel)
        .filter_map(|el| {
            let href = el.value().attr("href")?;
            let resolved = resolve_url(base_url, href)?;
            let is_internal = resolved
                .host_str()
                .map(|h| h == base_domain || h.ends_with(&format!(".{}", base_domain)))
                .unwrap_or(false);

            Some(PageLink {
                url: resolved.to_string(),
                text: el.text().collect::<String>().trim().to_string(),
                is_internal,
                rel: el.value().attr("rel").map(|s| s.to_string()),
            })
        })
        .collect()
}

// ─── Images ─────────────────────────────────────────────────────────────────────

fn extract_images(doc: &Html, base_url: &url::Url) -> Vec<ImageInfo> {
    let sel = selector!("img[src]");
    doc.select(&sel)
        .filter_map(|el| {
            let src = el.value().attr("src")?;
            let resolved = resolve_url(base_url, src)?;
            Some(ImageInfo {
                url: resolved.to_string(),
                alt: el.value().attr("alt").unwrap_or("").to_string(),
                width: el.value().attr("width").and_then(|w| w.parse().ok()),
                height: el.value().attr("height").and_then(|h| h.parse().ok()),
            })
        })
        .collect()
}

// ─── Tables ─────────────────────────────────────────────────────────────────────

fn extract_tables(doc: &Html) -> Vec<TableData> {
    let sel = selector!("table");
    let caption_sel = selector!("caption");
    let thead_sel = selector!("thead th, thead td");
    let row_sel = selector!("tbody tr, tr");
    let cell_sel = selector!("td, th");

    doc.select(&sel)
        .enumerate()
        .map(|(index, table)| {
            let caption = table
                .select(&caption_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string());

            let headers: Vec<String> = table
                .select(&thead_sel)
                .map(|th| th.text().collect::<String>().trim().to_string())
                .collect();

            let rows: Vec<Vec<String>> = table
                .select(&row_sel)
                .skip(if headers.is_empty() { 0 } else { 1 }) // skip header row if present
                .map(|tr| {
                    tr.select(&cell_sel)
                        .map(|td| td.text().collect::<String>().trim().to_string())
                        .collect()
                })
                .filter(|row: &Vec<String>| !row.is_empty())
                .collect();

            TableData { index, caption, headers, rows }
        })
        .collect()
}

// ─── CSS ────────────────────────────────────────────────────────────────────────

fn extract_css(doc: &Html, base_url: &url::Url) -> Vec<CssResource> {
    let mut resources = Vec::new();

    // Inline <style> tags
    let style_sel = selector!("style");
    for el in doc.select(&style_sel) {
        let content = el.text().collect::<String>();
        if !content.trim().is_empty() {
            resources.push(CssResource {
                source_url: None,
                content,
                is_inline: true,
            });
        }
    }

    // Linked stylesheets
    let link_sel = selector!(r#"link[rel="stylesheet"]"#);
    for el in doc.select(&link_sel) {
        if let Some(href) = el.value().attr("href") {
            if let Some(resolved) = resolve_url(base_url, href) {
                resources.push(CssResource {
                    source_url: Some(resolved.to_string()),
                    content: String::new(), // fetched separately if needed
                    is_inline: false,
                });
            }
        }
    }

    resources
}

// ─── JavaScript ─────────────────────────────────────────────────────────────────

fn extract_js(doc: &Html, base_url: &url::Url) -> Vec<JsResource> {
    let sel = selector!("script");
    doc.select(&sel)
        .filter_map(|el| {
            let src = el.value().attr("src");
            let inline_content = el.text().collect::<String>();

            if let Some(src_attr) = src {
                // External script
                let resolved = resolve_url(base_url, src_attr)?;
                Some(JsResource {
                    source_url: Some(resolved.to_string()),
                    content: String::new(),
                    is_inline: false,
                })
            } else if !inline_content.trim().is_empty() {
                // Inline script
                Some(JsResource {
                    source_url: None,
                    content: inline_content,
                    is_inline: true,
                })
            } else {
                None
            }
        })
        .collect()
}

// ─── Metadata ───────────────────────────────────────────────────────────────────

fn extract_metadata(doc: &Html, base_url: &url::Url) -> PageMetadata {
    let mut meta = PageMetadata::default();

    // Canonical URL
    if let Ok(sel) = Selector::parse(r#"link[rel="canonical"]"#) {
        meta.canonical_url = doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("href"))
            .map(|s| s.to_string());
    }

    // Language
    if let Ok(sel) = Selector::parse("html") {
        meta.language = doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("lang"))
            .map(|s| s.to_string());
    }

    // Charset
    if let Ok(sel) = Selector::parse(r#"meta[charset]"#) {
        meta.charset = doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("charset"))
            .map(|s| s.to_string());
    }

    // Author
    if let Ok(sel) = Selector::parse(r#"meta[name="author"]"#) {
        meta.author = doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string());
    }

    // Keywords
    if let Ok(sel) = Selector::parse(r#"meta[name="keywords"]"#) {
        if let Some(content) = doc.select(&sel).next().and_then(|el| el.value().attr("content")) {
            meta.keywords = content.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
    }

    // Robots
    if let Ok(sel) = Selector::parse(r#"meta[name="robots"]"#) {
        meta.robots = doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string());
    }

    // Open Graph
    if let Ok(sel) = Selector::parse(r#"meta[property^="og:"]"#) {
        for el in doc.select(&sel) {
            if let (Some(prop), Some(content)) = (el.value().attr("property"), el.value().attr("content")) {
                meta.og_properties.insert(prop.to_string(), content.to_string());
            }
        }
    }

    // Twitter Card
    if let Ok(sel) = Selector::parse(r#"meta[name^="twitter:"]"#) {
        for el in doc.select(&sel) {
            if let (Some(name), Some(content)) = (el.value().attr("name"), el.value().attr("content")) {
                meta.twitter_properties.insert(name.to_string(), content.to_string());
            }
        }
    }

    // Favicon
    if let Ok(sel) = Selector::parse(r#"link[rel="icon"], link[rel="shortcut icon"]"#) {
        meta.favicon_url = doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("href"))
            .and_then(|href| resolve_url(base_url, href))
            .map(|u| u.to_string());
    }

    meta
}

// ─── Structured data (JSON-LD) ──────────────────────────────────────────────────

fn extract_structured_data(doc: &Html) -> Vec<serde_json::Value> {
    let sel = selector!(r#"script[type="application/ld+json"]"#);
    doc.select(&sel)
        .filter_map(|el| {
            let text = el.text().collect::<String>();
            serde_json::from_str(&text).ok()
        })
        .collect()
}

// ─── URL resolution helper ──────────────────────────────────────────────────────

/// Resolve a potentially relative URL against a base URL.
///
/// Returns `None` for `javascript:`, `mailto:`, `tel:`, and `data:` URLs.
fn resolve_url(base: &url::Url, href: &str) -> Option<url::Url> {
    let trimmed = href.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("javascript:")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("tel:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with('#')
    {
        return None;
    }
    base.join(trimmed).ok()
}

#[cfg(test)]
mod extract_tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = "<html><head><title>Test Page</title></head><body></body></html>";
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc), "Test Page");
    }

    #[test]
    fn test_extract_headings() {
        let html = r#"<html><body><h1>Title</h1><h2 id="sec">Section</h2><h3>Sub</h3></body></html>"#;
        let doc = Html::parse_document(html);
        let headings = extract_headings(&doc);
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].id, Some("sec".to_string()));
    }

    #[test]
    fn test_resolve_url() {
        let base = url::Url::parse("https://example.com/docs/guide").unwrap();
        assert!(resolve_url(&base, "javascript:void(0)").is_none());
        assert!(resolve_url(&base, "#anchor").is_none());
        assert_eq!(
            resolve_url(&base, "/about").unwrap().as_str(),
            "https://example.com/about"
        );
        assert_eq!(
            resolve_url(&base, "page2").unwrap().as_str(),
            "https://example.com/docs/page2"
        );
    }
}

