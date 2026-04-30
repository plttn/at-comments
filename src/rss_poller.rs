use crate::settings::PollerConfig;
use tokio::time::Duration;
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

/// Extract slug from a blog URL (assumes URL has already been validated by find_blog_url).
/// Returns the last nonempty path segment.
fn extract_slug_from_url(url_str: &str) -> Option<String> {
    let url = Url::parse(url_str).ok()?;

    // Return the last nonempty path segment (handles trailing slashes).
    url.path_segments()?
        .rev()
        .find(|segment| !segment.is_empty())
        .map(|s| s.to_string())
}

/// Extract the blog URL from a post description.
/// Expected format: "📝 Title text ... https://domain/slug"
/// Returns the URL if it matches the blog domain, None otherwise.
fn find_blog_url(description: &str, target_emoji: &str, blog_domain: &str) -> Option<String> {
    if !description.starts_with(target_emoji) {
        return None;
    }

    // Get the last whitespace-separated token (expected to be the URL)
    description.split_whitespace().last().and_then(|url| {
        // Strip trailing punctuation
        let url = url.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != ':' && c != '/' && c != '.' && c != '-' && c != '_'
        });
        // Validate it's an HTTP(S) URL for our domain
        if (url.starts_with("http://") || url.starts_with("https://")) && url.contains(blog_domain)
        {
            Some(url.to_string())
        } else {
            None
        }
    })
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

        let url = match find_blog_url(description, &config.emoji, &config.domain) {
            Some(u) => u,
            None => continue,
        };

        let time_us = item
            .pub_date()
            .and_then(|date_str| chrono::DateTime::parse_from_rfc2822(date_str).ok())
            .map(|dt| dt.timestamp_micros())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_micros());

        if let Some(slug) = extract_slug_from_url(&url) {
            entries.push(PostEntry {
                slug,
                rkey,
                time_us,
            });
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

    let mut ticker = tokio::time::interval(Duration::from_secs(15 * 60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await; // fires immediately on first iteration, then every 15 mins

        if let Err(e) = poll_rss(&client, &pool, &config).await {
            log::error!("Poll failed: {}", e);
        }
    }
}
