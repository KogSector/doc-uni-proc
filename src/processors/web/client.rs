// --- Start of http_client.rs ---
//! HTTP client with connection pooling for URL processing
//!
//! This module provides an HTTP client with:
//! - Connection pooling (50 connections)
//! - Configurable timeout (5-120 seconds)
//! - Custom header support
//! - User agent configuration
//! - Automatic redirect handling (up to 5 levels)

use crate::processors::web::{ProcessingConfig, ProcessingError, Result};
use reqwest::{Client, ClientBuilder, Response};
use std::time::Duration;
use std::collections::HashMap;
use tracing::{debug, warn};

/// HTTP client with connection pooling and configuration
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
}

/// Configuration for the HTTP client
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout in seconds (5-120)
    pub timeout_seconds: u64,
    /// Connection pool size
    pub pool_max_idle_per_host: usize,
    /// Custom HTTP headers
    pub custom_headers: std::collections::HashMap<String, String>,
    /// User agent string
    pub user_agent: String,
    /// Maximum number of redirects to follow
    pub max_redirects: usize,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            pool_max_idle_per_host: 50,
            custom_headers: std::collections::HashMap::new(),
            user_agent: "ConFuseBot/1.0 (+https://confuse.ai/bot)".to_string(),
            max_redirects: 5,
        }
    }
}

impl HttpClientConfig {
    /// Create configuration from ProcessingConfig
    pub fn from_processing_config(config: &ProcessingConfig) -> Self {
        Self {
            timeout_seconds: config.timeout_seconds,
            pool_max_idle_per_host: 50, // Fixed at 50 as per requirements
            custom_headers: config.custom_headers.clone(),
            user_agent: config.user_agent.clone(),
            max_redirects: 5, // Fixed at 5 as per requirements
        }
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.timeout_seconds < 5 || self.timeout_seconds > 120 {
            return Err(ProcessingError::ConfigurationError(
                format!(
                    "Timeout must be between 5 and 120 seconds, got {}",
                    self.timeout_seconds
                )
            ));
        }
        Ok(())
    }
}

impl HttpClient {
    /// Create a new HTTP client with the given configuration
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        config.validate()?;

        debug!(
            "Creating HTTP client with timeout={}s, pool_size={}, user_agent={}",
            config.timeout_seconds, config.pool_max_idle_per_host, config.user_agent
        );

        let mut builder = ClientBuilder::new()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .user_agent(&config.user_agent)
            .redirect(reqwest::redirect::Policy::limited(config.max_redirects))
            .gzip(true)
            .brotli(true);

        // Add custom headers as default headers
        if !config.custom_headers.is_empty() {
            let mut headers = reqwest::header::HeaderMap::new();
            for (key, value) in &config.custom_headers {
                match reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                    Ok(header_name) => {
                        match reqwest::header::HeaderValue::from_str(value) {
                            Ok(header_value) => {
                                headers.insert(header_name, header_value);
                            }
                            Err(e) => {
                                warn!("Invalid header value for '{}': {}", key, e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Invalid header name '{}': {}", key, e);
                    }
                }
            }
            builder = builder.default_headers(headers);
        }

        let client = builder
            .build()
            .map_err(|e| ProcessingError::ConfigurationError(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// Create a new HTTP client from ProcessingConfig
    pub fn from_processing_config(config: &ProcessingConfig) -> Result<Self> {
        let http_config = HttpClientConfig::from_processing_config(config);
        Self::new(http_config)
    }

    /// Perform a GET request to the specified URL
    pub async fn get(&self, url: &str) -> Result<Response> {
        debug!("GET request to: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProcessingError::TimeoutError(format!("Request to {} timed out after {}s", url, self.config.timeout_seconds))
                } else if e.is_connect() {
                    ProcessingError::NetworkError(format!("Connection failed to {}: {}", url, e))
                } else {
                    ProcessingError::HttpError(e)
                }
            })?;

        debug!(
            "Response from {}: status={}, content_type={:?}",
            url,
            response.status(),
            response.headers().get(reqwest::header::CONTENT_TYPE)
        );

        Ok(response)
    }

    /// Perform a GET request and return the response body as text
    pub async fn get_text(&self, url: &str) -> Result<String> {
        let response = self.get(url).await?;
        
        let status = response.status();
        if !status.is_success() {
            return Err(ProcessingError::HttpError(
                response.error_for_status().unwrap_err()
            ));
        }

        let text = response
            .text()
            .await
            .map_err(|e| ProcessingError::NetworkError(format!("Failed to read response body: {}", e)))?;

        Ok(text)
    }

    /// Perform a GET request and return the response body as bytes
    pub async fn get_bytes(&self, url: &str) -> Result<bytes::Bytes> {
        let response = self.get(url).await?;
        
        let status = response.status();
        if !status.is_success() {
            return Err(ProcessingError::HttpError(
                response.error_for_status().unwrap_err()
            ));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| ProcessingError::NetworkError(format!("Failed to read response body: {}", e)))?;

        Ok(bytes)
    }

    /// Get the configuration
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// Get the underlying reqwest client
    pub fn inner(&self) -> &Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.pool_max_idle_per_host, 50);
        assert_eq!(config.max_redirects, 5);
        assert!(config.custom_headers.is_empty());
    }

    #[test]
    fn test_config_validation() {
        let mut config = HttpClientConfig::default();
        
        // Valid timeout
        config.timeout_seconds = 30;
        assert!(config.validate().is_ok());

        // Timeout too low
        config.timeout_seconds = 4;
        assert!(config.validate().is_err());

        // Timeout too high
        config.timeout_seconds = 121;
        assert!(config.validate().is_err());

        // Boundary values
        config.timeout_seconds = 5;
        assert!(config.validate().is_ok());
        
        config.timeout_seconds = 120;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_from_processing_config() {
        let mut processing_config = ProcessingConfig::default();
        processing_config.timeout_seconds = 60;
        processing_config.user_agent = "TestBot/1.0".to_string();
        processing_config.custom_headers.insert("X-Custom".to_string(), "value".to_string());

        let http_config = HttpClientConfig::from_processing_config(&processing_config);
        
        assert_eq!(http_config.timeout_seconds, 60);
        assert_eq!(http_config.user_agent, "TestBot/1.0");
        assert_eq!(http_config.custom_headers.get("X-Custom"), Some(&"value".to_string()));
        assert_eq!(http_config.pool_max_idle_per_host, 50);
        assert_eq!(http_config.max_redirects, 5);
    }

    #[test]
    fn test_client_creation() {
        let config = HttpClientConfig::default();
        let client = HttpClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_with_custom_headers() {
        let mut config = HttpClientConfig::default();
        config.custom_headers.insert("X-API-Key".to_string(), "secret123".to_string());
        config.custom_headers.insert("X-Request-ID".to_string(), "req-456".to_string());

        let client = HttpClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_with_invalid_timeout() {
        let mut config = HttpClientConfig::default();
        config.timeout_seconds = 200; // Too high

        let client = HttpClient::new(config);
        assert!(client.is_err());
    }
}

// --- Start of robots.rs ---
/// Robots.txt parsing and compliance checking
///
/// This module provides functionality to fetch, parse, and check URLs against robots.txt rules.
/// It implements the Robots Exclusion Protocol as specified in RFC 9309.


use url::Url;

/// Represents a parsed robots.txt file
#[derive(Debug, Clone)]
pub struct RobotsTxt {
    /// Rules organized by user agent
    rules: HashMap<String, UserAgentRules>,
    /// Default rules for all user agents (*)
    default_rules: UserAgentRules,
}

/// Rules for a specific user agent
#[derive(Debug, Clone, Default)]
struct UserAgentRules {
    /// Disallowed paths
    disallow: Vec<String>,
    /// Allowed paths (takes precedence over disallow)
    allow: Vec<String>,
    /// Crawl delay in seconds
    crawl_delay: Option<Duration>,
}

impl RobotsTxt {
    /// Fetch and parse robots.txt from a domain
    pub async fn fetch(client: &Client, base_url: &str) -> Result<Self> {
        let url = Self::robots_url(base_url)?;
        
        // Fetch robots.txt with a timeout
        let response = client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let content = resp.text().await.map_err(|e| {
                        ProcessingError::NetworkError(format!("Failed to read robots.txt: {}", e))
                    })?;
                    Ok(Self::parse(&content))
                } else {
                    // If robots.txt doesn't exist or returns an error, allow all
                    Ok(Self::allow_all())
                }
            }
            Err(e) => {
                // If we can't fetch robots.txt (network error, timeout, etc.), allow all
                tracing::warn!("Failed to fetch robots.txt from {}: {}. Allowing all URLs.", url, e);
                Ok(Self::allow_all())
            }
        }
    }

    /// Parse robots.txt content
    pub fn parse(content: &str) -> Self {
        let mut rules: HashMap<String, UserAgentRules> = HashMap::new();
        let mut default_rules = UserAgentRules::default();
        let mut current_agents: Vec<String> = Vec::new();

        for line in content.lines() {
            // Remove comments
            let line = line.split('#').next().unwrap_or("").trim();
            
            if line.is_empty() {
                continue;
            }

            // Parse directive
            if let Some((directive, value)) = line.split_once(':') {
                let directive = directive.trim().to_lowercase();
                let value = value.trim();

                match directive.as_str() {
                    "user-agent" => {
                        // Start a new user agent section
                        if !current_agents.is_empty() {
                            // Save previous rules
                            Self::save_rules(&mut rules, &mut default_rules, &current_agents);
                        }
                        current_agents.clear();
                        current_agents.push(value.to_lowercase());
                    }
                    "disallow" => {
                        if current_agents.is_empty() {
                            current_agents.push("*".to_string());
                        }
                        for agent in &current_agents {
                            let agent_rules = rules.entry(agent.clone()).or_default();
                            if !value.is_empty() {
                                agent_rules.disallow.push(value.to_string());
                            }
                        }
                    }
                    "allow" => {
                        if current_agents.is_empty() {
                            current_agents.push("*".to_string());
                        }
                        for agent in &current_agents {
                            let agent_rules = rules.entry(agent.clone()).or_default();
                            if !value.is_empty() {
                                agent_rules.allow.push(value.to_string());
                            }
                        }
                    }
                    "crawl-delay" => {
                        if let Ok(delay) = value.parse::<f64>() {
                            let duration = Duration::from_secs_f64(delay);
                            for agent in &current_agents {
                                let agent_rules = rules.entry(agent.clone()).or_default();
                                agent_rules.crawl_delay = Some(duration);
                            }
                        }
                    }
                    _ => {
                        // Ignore unknown directives
                    }
                }
            }
        }

        // Extract default rules for "*"
        if let Some(rules_for_all) = rules.remove("*") {
            default_rules = rules_for_all;
        }

        Self {
            rules,
            default_rules,
        }
    }

    /// Create a RobotsTxt that allows all URLs
    pub fn allow_all() -> Self {
        Self {
            rules: HashMap::new(),
            default_rules: UserAgentRules::default(),
        }
    }

    /// Check if a URL is allowed for a given user agent
    pub fn is_allowed(&self, url: &str, user_agent: &str) -> bool {
        let parsed_url = match Url::parse(url) {
            Ok(u) => u,
            Err(_) => return false, // Invalid URLs are not allowed
        };

        let path = parsed_url.path();
        
        // Get rules for this user agent, falling back to default rules
        let agent_key = user_agent.to_lowercase();
        let rules = self.rules.get(&agent_key).unwrap_or(&self.default_rules);

        // Check allow rules first (they take precedence)
        for allow_pattern in &rules.allow {
            if Self::matches_pattern(path, allow_pattern) {
                return true;
            }
        }

        // Check disallow rules
        for disallow_pattern in &rules.disallow {
            if Self::matches_pattern(path, disallow_pattern) {
                return false;
            }
        }

        // If no rules match, allow by default
        true
    }

    /// Get the crawl delay for a user agent
    pub fn crawl_delay(&self, user_agent: &str) -> Option<Duration> {
        let agent_key = user_agent.to_lowercase();
        self.rules
            .get(&agent_key)
            .and_then(|r| r.crawl_delay)
            .or(self.default_rules.crawl_delay)
    }

    /// Construct the robots.txt URL from a base URL
    fn robots_url(base_url: &str) -> Result<String> {
        let parsed = Url::parse(base_url).map_err(|e| {
            ProcessingError::ValidationError(format!("Invalid base URL: {}", e))
        })?;

        let scheme = parsed.scheme();
        let host = parsed.host_str().ok_or_else(|| {
            ProcessingError::ValidationError("URL has no host".to_string())
        })?;
        let port = parsed.port();

        let robots_url = if let Some(port) = port {
            format!("{}://{}:{}/robots.txt", scheme, host, port)
        } else {
            format!("{}://{}/robots.txt", scheme, host)
        };

        Ok(robots_url)
    }

    /// Check if a path matches a robots.txt pattern
    fn matches_pattern(path: &str, pattern: &str) -> bool {
        // Handle empty pattern
        if pattern.is_empty() {
            return true;
        }

        let must_match_end = pattern.ends_with('$');
        let pattern = if must_match_end {
            &pattern[..pattern.len() - 1]
        } else {
            pattern
        };

        // Handle exact match
        if path == pattern {
            return true;
        }

        // Handle prefix match (most common case)
        if let Some(prefix) = pattern.strip_suffix('*') {
            if must_match_end {
                return path == prefix; // Because it ends with * and $, it effectively means exact match
            }
            return path.starts_with(prefix);
        }

        // Handle suffix match
        if let Some(suffix) = pattern.strip_prefix('*') {
            if must_match_end {
                return path.ends_with(suffix);
            } else {
                return path.contains(suffix);
            }
        }

        // Handle wildcard in the middle
        if pattern.contains('*') {
            return Self::matches_wildcard(path, pattern, must_match_end);
        }

        // Default: check if path starts with pattern
        if must_match_end {
            path == pattern
        } else {
            path.starts_with(pattern)
        }
    }

    /// Match a path against a pattern with wildcards
    fn matches_wildcard(path: &str, pattern: &str, must_match_end: bool) -> bool {
        let parts: Vec<&str> = pattern.split('*').collect();
        
        if parts.is_empty() {
            return true;
        }

        let mut pos = 0;
        
        // First part must match at the beginning
        if !parts[0].is_empty() {
            if !path[pos..].starts_with(parts[0]) {
                return false;
            }
            pos += parts[0].len();
        }

        // Middle parts
        for part in &parts[1..parts.len() - 1] {
            if part.is_empty() {
                continue;
            }
            if let Some(idx) = path[pos..].find(part) {
                pos += idx + part.len();
            } else {
                return false;
            }
        }

        // Last part
        if let Some(last) = parts.last() {
            if !last.is_empty() {
                if must_match_end {
                    return path[pos..].ends_with(last);
                } else {
                    return path[pos..].contains(last);
                }
            }
        }

        true
    }

    /// Helper to save rules for current user agents
    fn save_rules(
        _rules: &mut HashMap<String, UserAgentRules>,
        _default_rules: &mut UserAgentRules,
        _current_agents: &[String],
    ) {
        // This is called when we encounter a new user-agent line
        // In the current implementation, rules are added directly in the parse loop
        // This function is kept for potential future use
    }
}

#[cfg(test)]
mod robots_tests {
    use super::*;

    #[test]
    fn test_parse_simple_robots_txt() {
        let content = r#"
User-agent: *
Disallow: /admin/
Disallow: /private/
Allow: /public/
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(!robots.is_allowed("https://example.com/admin/page", "TestBot"));
        assert!(!robots.is_allowed("https://example.com/private/data", "TestBot"));
        assert!(robots.is_allowed("https://example.com/public/info", "TestBot"));
        assert!(robots.is_allowed("https://example.com/other/page", "TestBot"));
    }

    #[test]
    fn test_parse_specific_user_agent() {
        let content = r#"
User-agent: Googlebot
Disallow: /nogoogle/

User-agent: *
Disallow: /admin/
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(!robots.is_allowed("https://example.com/nogoogle/page", "Googlebot"));
        assert!(robots.is_allowed("https://example.com/admin/page", "Googlebot"));
        assert!(!robots.is_allowed("https://example.com/admin/page", "OtherBot"));
    }

    #[test]
    fn test_allow_takes_precedence() {
        let content = r#"
User-agent: *
Disallow: /private/
Allow: /private/public/
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(robots.is_allowed("https://example.com/private/public/page", "TestBot"));
        assert!(!robots.is_allowed("https://example.com/private/secret", "TestBot"));
    }

    #[test]
    fn test_wildcard_patterns() {
        let content = r#"
User-agent: *
Disallow: /*.pdf$
Disallow: /temp*
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(!robots.is_allowed("https://example.com/document.pdf", "TestBot"));
        assert!(!robots.is_allowed("https://example.com/temp", "TestBot"));
        assert!(!robots.is_allowed("https://example.com/temporary", "TestBot"));
        assert!(robots.is_allowed("https://example.com/document.html", "TestBot"));
    }

    #[test]
    fn test_crawl_delay() {
        let content = r#"
User-agent: SlowBot
Crawl-delay: 5

User-agent: *
Crawl-delay: 1
"#;

        let robots = RobotsTxt::parse(content);
        
        assert_eq!(robots.crawl_delay("SlowBot"), Some(Duration::from_secs(5)));
        assert_eq!(robots.crawl_delay("OtherBot"), Some(Duration::from_secs(1)));
    }

    #[test]
    fn test_empty_disallow_allows_all() {
        let content = r#"
User-agent: *
Disallow:
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(robots.is_allowed("https://example.com/anything", "TestBot"));
        assert!(robots.is_allowed("https://example.com/admin/", "TestBot"));
    }

    #[test]
    fn test_case_insensitive_user_agent() {
        let content = r#"
User-agent: TestBot
Disallow: /private/
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(!robots.is_allowed("https://example.com/private/page", "testbot"));
        assert!(!robots.is_allowed("https://example.com/private/page", "TESTBOT"));
        assert!(!robots.is_allowed("https://example.com/private/page", "TestBot"));
    }

    #[test]
    fn test_comments_ignored() {
        let content = r#"
# This is a comment
User-agent: * # inline comment
Disallow: /admin/ # another comment
"#;

        let robots = RobotsTxt::parse(content);
        
        assert!(!robots.is_allowed("https://example.com/admin/page", "TestBot"));
    }

    #[test]
    fn test_robots_url_construction() {
        assert_eq!(
            RobotsTxt::robots_url("https://example.com").unwrap(),
            "https://example.com/robots.txt"
        );
        assert_eq!(
            RobotsTxt::robots_url("https://example.com/path/to/page").unwrap(),
            "https://example.com/robots.txt"
        );
        assert_eq!(
            RobotsTxt::robots_url("https://example.com:8080").unwrap(),
            "https://example.com:8080/robots.txt"
        );
    }

    #[test]
    fn test_allow_all() {
        let robots = RobotsTxt::allow_all();
        
        assert!(robots.is_allowed("https://example.com/anything", "TestBot"));
        assert!(robots.is_allowed("https://example.com/admin/", "TestBot"));
        assert!(robots.is_allowed("https://example.com/private/", "TestBot"));
    }

    #[test]
    fn test_matches_pattern_prefix() {
        assert!(RobotsTxt::matches_pattern("/admin/page", "/admin/"));
        assert!(RobotsTxt::matches_pattern("/admin/", "/admin/"));
        assert!(!RobotsTxt::matches_pattern("/other/page", "/admin/"));
    }

    #[test]
    fn test_matches_pattern_wildcard() {
        assert!(RobotsTxt::matches_pattern("/admin/page.html", "/admin/*.html"));
        assert!(RobotsTxt::matches_pattern("/page.pdf", "*.pdf"));
        assert!(RobotsTxt::matches_pattern("/temp/file", "/temp*"));
        assert!(!RobotsTxt::matches_pattern("/page.html", "*.pdf"));
    }

    #[test]
    fn test_invalid_url_not_allowed() {
        let robots = RobotsTxt::allow_all();
        assert!(!robots.is_allowed("not a valid url", "TestBot"));
    }
}

