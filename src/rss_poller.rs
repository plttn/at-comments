use rocket::Config;
use rocket_db_pools::sqlx::{self};
use serde::Deserialize;
use tokio::time::{sleep, Duration};

#[derive(Deserialize, Debug)]
struct PollerConfig {
    poster_handle: String,
    target_emoji: String,
    blog_domain: String,
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
    log::info!("[rss] Polling RSS feed for {}", config.poster_handle);

    let channel = fetch_rss(&config.poster_handle).await?;

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
                log::warn!("[rss] Failed to extract rkey from guid: {}", guid);
                continue;
            }
        };

        // Get post description/content
        let description = match item.description() {
            Some(d) => d,
            None => continue,
        };

        // Check for target emoji and blog URLs
        let urls = find_blog_urls(description, &config.target_emoji, &config.blog_domain);

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
            if let Some(slug) = extract_slug_from_url(&url, &config.blog_domain) {
                match sqlx::query(
                    "INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING"
                )
                .bind(&slug)
                .bind(&rkey)
                .bind(&time_us)
                .execute(pool)
                .await
                {
                    Ok(result) => {
                        if result.rows_affected() > 0 {
                            log::info!("[rss] Inserted new post: slug={}, rkey={}", slug, rkey);
                            processed += 1;
                        }
                    }
                    Err(e) => {
                        log::error!("[rss] Failed to insert post {}: {}", slug, e);
                    }
                }
            }
        }
    }

    log::info!("[rss] Poll complete, processed {} new posts", processed);
    Ok(())
}

/// Background task that polls RSS every 15 minutes
pub async fn rss_polling_task(pool: sqlx::Pool<sqlx::Postgres>) {
    let config = match Config::figment().extract::<PollerConfig>() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[rss] Failed to load config: {}", e);
            return;
        }
    };

    log::info!(
        "[rss] Starting RSS poller for {} (emoji: {}, domain: {})",
        config.poster_handle,
        config.target_emoji,
        config.blog_domain
    );

    // Do initial poll immediately
    if let Err(e) = poll_rss(&pool, &config).await {
        log::error!("[rss] Initial poll failed: {}", e);
    }

    // Poll every 15 minutes
    let interval = Duration::from_secs(15 * 60);

    loop {
        sleep(interval).await;

        if let Err(e) = poll_rss(&pool, &config).await {
            log::error!("[rss] Poll failed: {}", e);
        }
    }
}
