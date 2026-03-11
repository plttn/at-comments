use serde::Deserialize;
use tokio::time::{sleep, Duration};

thread_local! {
    static POLLER_CONFIG: std::cell::RefCell<Option<PollerConfig>> = const { std::cell::RefCell::new(None) };
}

#[derive(Deserialize, Debug, Clone)]
pub struct PollerConfig {
    pub handle: String,
    pub emoji: String,
    pub domain: String,
}

impl PollerConfig {
    /// Load poller config from environment variables
    pub fn from_env() -> Result<Self, String> {
        let cfg =
            crate::settings::build_config().map_err(|e| format!("Failed to load config: {}", e))?;

        let poster_handle = cfg
            .get_string("poller.handle")
            .map_err(|_| "ATC_POLLER_HANDLE not set".to_string())?;
        let target_emoji = cfg
            .get_string("poller.emoji")
            .map_err(|_| "ATC_POLLER_EMOJI not set".to_string())?;
        let blog_domain = cfg
            .get_string("poller.domain")
            .map_err(|_| "ATC_POLLER_DOMAIN not set".to_string())?;

        Ok(PollerConfig {
            handle: poster_handle,
            emoji: target_emoji,
            domain: blog_domain,
        })
    }
}

/// Fetch and parse RSS feed from Bluesky profile
async fn fetch_rss(handle: &str) -> Result<rss::Channel, String> {
    let url = format!("https://bsky.app/profile/{}/rss", handle);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch RSS: {}", e))?;

    let content = response
        .text()
        .await
        .map_err(|e| format!("Failed to read RSS content: {}", e))?;

    rss::Channel::read_from(content.as_bytes()).map_err(|e| format!("Failed to parse RSS: {}", e))
}

/// Extract rkey from Bluesky post AT-URI
/// Format: at://did:plc:xxx/app.bsky.feed.post/rkey
fn extract_rkey(uri: &str) -> Option<String> {
    uri.split('/').next_back().map(|s| s.to_string())
}

/// Extract slug from blog URL
fn extract_slug_from_url(url: &str, blog_domain: &str) -> Option<String> {
    if !url.contains(blog_domain) {
        return None;
    }

    // Remove query/fragment and normalize trailing slash URLs.
    let without_query = url.split('?').next().unwrap_or(url);
    let without_fragment = without_query.split('#').next().unwrap_or(without_query);
    let normalized = without_fragment.trim_end_matches('/');

    normalized
        .split('/')
        .next_back()
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

/// Check post text for target emoji and extract blog URLs
fn find_blog_urls(description: &str, target_emoji: &str, blog_domain: &str) -> Vec<String> {
    if !description.starts_with(target_emoji) {
        return vec![];
    }

    // Simple URL extraction - look for blog domain in text
    description
        .split_whitespace()
        .filter(|word| word.contains(blog_domain))
        .map(|s| {
            s.trim_matches(|c: char| {
                !c.is_alphanumeric() && c != ':' && c != '/' && c != '.' && c != '-' && c != '_'
            })
        })
        .filter(|s| s.starts_with("http"))
        .map(|s| s.to_string())
        .collect()
}

/// Poll RSS feed and update database
async fn poll_rss(pool: &sqlx::Pool<sqlx::Postgres>, config: &PollerConfig) -> Result<(), String> {
    log::info!("Polling RSS feed for {}", config.handle);

    let channel = fetch_rss(&config.handle).await?;

    let mut processed = 0;

    for item in channel.items() {
        // Extract rkey from guid (contains at:// URI)
        let guid = match item.guid() {
            Some(g) => g.value(),
            None => continue,
        };

        let rkey = match extract_rkey(guid) {
            Some(r) => r,
            None => {
                log::warn!("Failed to extract rkey from guid: {}", guid);
                continue;
            }
        };

        // Get post description/content
        let description = match item.description() {
            Some(d) => d,
            None => continue,
        };

        // Check for target emoji and blog URLs
        let urls = find_blog_urls(description, &config.emoji, &config.domain);

        if urls.is_empty() {
            continue;
        }

        // Extract timestamp from pub_date if available
        let time_us = item
            .pub_date()
            .and_then(|date_str| chrono::DateTime::parse_from_rfc2822(date_str).ok())
            .map(|dt| dt.timestamp_micros().to_string())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_micros().to_string());

        // Process each blog URL found
        for url in urls {
            if let Some(slug) = extract_slug_from_url(&url, &config.domain) {
                let insert_result = sqlx::query(
                    "INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING"
                )
                .bind(&slug)
                .bind(&rkey)
                .bind(&time_us)
                .execute(pool)
                .await;

                match insert_result {
                    Ok(result) => {
                        if result.rows_affected() > 0 {
                            log::info!("Inserted new post: slug={}, rkey={}", slug, rkey);
                            processed += 1;
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to insert post {}: {}", slug, e);
                    }
                }
            }
        }
    }

    log::info!("Poll complete, processed {} new posts", processed);
    Ok(())
}

/// Look up a specific slug in the RSS feed on demand.
/// Returns `(rkey, time_us)` if the slug is found, `None` otherwise.
pub async fn lookup_slug_in_rss(slug: &str) -> Option<(String, String)> {
    let config = match PollerConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return None;
        }
    };

    let channel = match fetch_rss(&config.handle).await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to fetch RSS for on-demand lookup: {}", e);
            return None;
        }
    };

    for item in channel.items() {
        let guid = match item.guid() {
            Some(g) => g.value(),
            None => continue,
        };

        let rkey = match extract_rkey(guid) {
            Some(r) => r,
            None => continue,
        };

        let description = match item.description() {
            Some(d) => d,
            None => continue,
        };

        let urls = find_blog_urls(description, &config.emoji, &config.domain);

        for url in &urls {
            if let Some(found_slug) = extract_slug_from_url(url, &config.domain) {
                if found_slug == slug {
                    let time_us = item
                        .pub_date()
                        .and_then(|date_str| chrono::DateTime::parse_from_rfc2822(date_str).ok())
                        .map(|dt| dt.timestamp_micros().to_string())
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_micros().to_string());
                    log::info!("On-demand lookup found slug={} rkey={}", slug, rkey);
                    return Some((rkey, time_us));
                }
            }
        }
    }

    None
}

/// Background task that polls RSS every 15 minutes
pub async fn rss_polling_task(pool: sqlx::Pool<sqlx::Postgres>) {
    let config = match PollerConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return;
        }
    };

    log::info!(
        "Starting RSS poller for {} (emoji: {}, domain: {})",
        config.handle,
        config.emoji,
        config.domain
    );

    // Do initial poll immediately
    if let Err(e) = poll_rss(&pool, &config).await {
        log::error!("Initial poll failed: {}", e);
    }

    // Poll every 15 minutes
    let interval = Duration::from_secs(15 * 60);

    loop {
        sleep(interval).await;

        if let Err(e) = poll_rss(&pool, &config).await {
            log::error!("Poll failed: {}", e);
        }
    }
}
