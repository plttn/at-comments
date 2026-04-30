use crate::settings::PollerConfig;
use tokio::time::{sleep, Duration};
use url::Url;

/// A parsed post entry extracted from an RSS item.
struct PostEntry {
    slug: String,
    rkey: String,
    time_us: i64,
}

/// Fetch and parse RSS feed from Bluesky profile
async fn fetch_rss(client: &reqwest::Client, handle: &str) -> Result<rss::Channel, String> {
    let url = format!("https://bsky.app/profile/{}/rss", handle);

    let response = client
        .get(&url)
        .send()
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

/// Extract slug from blog URL using `url::Url` for robust parsing
fn extract_slug_from_url(url_str: &str, blog_domain: &str) -> Option<String> {
    // Try parsing as-is, and fall back to adding https:// if no scheme is present.
    let url = Url::parse(url_str)
        .or_else(|_| Url::parse(&format!("https://{}", url_str)))
        .ok()?;

    // Ensure the host/domain matches. Allow subdomains by using ends_with.
    let domain = url.domain()?;
    if !domain.ends_with(blog_domain) {
        return None;
    }

    // Return the last nonempty path segment (handles trailing slashes).
    url.path_segments()?
        .rev()
        .find(|segment| !segment.is_empty())
        .map(|s| s.to_string())
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
        .filter(|s| s.starts_with("http") || s.contains("/"))
        .map(|s| s.to_string())
        .collect()
}

/// Parse all matching post entries from an RSS channel.
/// Returns one `PostEntry` per (slug, rkey) pair found.
fn parse_rss_items(channel: &rss::Channel, config: &PollerConfig) -> Vec<PostEntry> {
    let mut entries = Vec::new();

    for item in channel.items() {
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

        let description = match item.description() {
            Some(d) => d,
            None => continue,
        };

        let urls = find_blog_urls(description, &config.emoji, &config.domain);
        if urls.is_empty() {
            continue;
        }

        let time_us = item
            .pub_date()
            .and_then(|date_str| chrono::DateTime::parse_from_rfc2822(date_str).ok())
            .map(|dt| dt.timestamp_micros())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_micros());

        for url in &urls {
            if let Some(slug) = extract_slug_from_url(url, &config.domain) {
                entries.push(PostEntry {
                    slug,
                    rkey: rkey.clone(),
                    time_us,
                });
            }
        }
    }

    entries
}

/// Poll RSS feed and update database
async fn poll_rss(
    client: &reqwest::Client,
    pool: &sqlx::Pool<sqlx::Postgres>,
    config: &PollerConfig,
) -> Result<(), String> {
    log::info!("Polling RSS feed for {}", config.handle);

    let channel = fetch_rss(client, &config.handle).await?;
    let entries = parse_rss_items(&channel, config);
    let mut processed = 0;

    for entry in entries {
        let insert_result = sqlx::query(
            "INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING"
        )
        .bind(&entry.slug)
        .bind(&entry.rkey)
        .bind(entry.time_us)
        .execute(pool)
        .await;

        match insert_result {
            Ok(result) => {
                if result.rows_affected() > 0 {
                    log::info!(
                        "Inserted new post: slug={}, rkey={}",
                        entry.slug,
                        entry.rkey
                    );
                    processed += 1;
                }
            }
            Err(e) => {
                log::error!("Failed to insert post {}: {}", entry.slug, e);
            }
        }
    }

    log::info!("Poll complete, processed {} new posts", processed);
    Ok(())
}

/// Look up a specific slug in the RSS feed on demand.
/// Returns `(rkey, time_us)` if the slug is found, `None` otherwise.
pub async fn lookup_slug_in_rss(
    client: &reqwest::Client,
    slug: &str,
    config: &PollerConfig,
) -> Option<(String, i64)> {
    let channel = match fetch_rss(client, &config.handle).await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to fetch RSS for on-demand lookup: {}", e);
            return None;
        }
    };

    let entries = parse_rss_items(&channel, config);

    entries.into_iter().find(|e| e.slug == slug).map(|e| {
        log::info!("On-demand lookup found slug={} rkey={}", slug, e.rkey);
        (e.rkey, e.time_us)
    })
}

/// Background task that polls RSS every 15 minutes
pub async fn rss_polling_task(
    client: reqwest::Client,
    pool: sqlx::Pool<sqlx::Postgres>,
    config: PollerConfig,
) {
    log::info!(
        "Starting RSS poller for {} (emoji: {}, domain: {})",
        config.handle,
        config.emoji,
        config.domain
    );

    // Do initial poll immediately
    if let Err(e) = poll_rss(&client, &pool, &config).await {
        log::error!("Initial poll failed: {}", e);
    }

    // Poll every 15 minutes
    let interval = Duration::from_secs(15 * 60);

    loop {
        sleep(interval).await;

        if let Err(e) = poll_rss(&client, &pool, &config).await {
            log::error!("Poll failed: {}", e);
        }
    }
}
