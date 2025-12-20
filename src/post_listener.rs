use atrium_api::app::bsky::richtext::facet::MainFeaturesItem;
use atrium_api::record::KnownRecord::AppBskyFeedPost;
use jetstream_oxide::{
    events::{commit::CommitEvent, JetstreamEvent::Commit},
    exports::{Did, Nsid},
    DefaultJetstreamEndpoints, JetstreamCompression, JetstreamConfig, JetstreamConnector,
};

use rocket::Config;
use rocket_db_pools::sqlx::{self};
use serde::Deserialize;
use sqlx::Row;

use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};
use url::Url;

use std::time::Instant;

#[derive(Deserialize, Debug)]
struct ListenerConfig {
    poster_did: String,
    target_emoji: String,
}

/// Quick TCP reachability check for a websocket `wss://` endpoint.
/// It attempts a TCP connect to the host:port within `timeout_ms` milliseconds.
async fn check_ws_endpoint_reachable(endpoint: &str, timeout_ms: u64) -> Result<(), String> {
    let url = Url::parse(endpoint).map_err(|e| format!("Bad endpoint URL: {}", e))?;
    let host = url
        .host_str()
        .ok_or_else(|| "No host in endpoint URL".to_string())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "No port and no default".to_string())?;

    let addr = format!("{host}:{port}");
    let dur = Duration::from_millis(timeout_ms);
    match timeout(dur, TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => Ok(()),
        Ok(Err(e)) => Err(format!("TCP connect to {addr} failed: {}", e)),
        Err(_) => Err(format!(
            "TCP connect to {addr} timed out after {}ms",
            timeout_ms
        )),
    }
}

/// Consumer-side Jetstream listener with preflight + backoff + endpoint toggling.
/// This keeps the consumer API unchanged (no jetstream-oxide edits required).
pub async fn websocket_listener(pool: sqlx::Pool<sqlx::Postgres>) {
    let listener_config = Config::figment().extract::<ListenerConfig>().unwrap();

    let nsid = vec![Nsid::new("app.bsky.feed.post".to_string()).unwrap()];
    let did = vec![Did::new(listener_config.poster_did.clone()).unwrap()];

    // Endpoints to rotate between on failures.
    let endpoints: Vec<String> = vec![
        DefaultJetstreamEndpoints::USWestOne.into(),
        DefaultJetstreamEndpoints::USEastTwo.into(),
    ];
    let mut endpoint_index: usize = 0;

    // Backoff parameters
    let mut retry_attempt: u32 = 0;
    let base_delay_ms: u64 = 1_000; // 1s
    let max_delay_ms: u64 = 30_000; // 30s
    let success_threshold_s: u64 = 15; // reset retries if connection lasted this long

    loop {
        // Recompute last persisted cursor from DB before each attempt (so we resume where we left off).
        let mut cursor = match sqlx::query("SELECT time_us FROM posts ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
        {
            Ok(row) => {
                let time = row.get::<String, _>(0);
                chrono::DateTime::from_timestamp_micros(time.parse::<i64>().unwrap())
            }
            Err(_) => None,
        };

        // If the cursor is older than a day, drop it to avoid massive replay.
        let diff = match cursor {
            Some(c) => chrono::Utc::now() - c,
            None => chrono::Duration::days(1),
        };
        if diff.num_days() >= 1 {
            log::warn!("[jetstream] Cursor is more than a day old, resetting cursor");
            cursor = None;
        }

        // Preflight TCP check to avoid returning a connector when host is obviously unreachable.
        let endpoint_str = endpoints[endpoint_index].as_str();
        match check_ws_endpoint_reachable(endpoint_str, 1500).await {
            Ok(()) => {
                log::warn!("[jetstream] Endpoint {} reachable (TCP)", endpoint_str);
            }
            Err(err) => {
                log::warn!(
                    "[jetstream] Endpoint {} not reachable: {}. toggling endpoint and backing off",
                    endpoint_str,
                    err
                );
                // Toggle endpoint and backoff
                endpoint_index = (endpoint_index + 1) % endpoints.len();
                let delay =
                    (base_delay_ms * (2_u64.saturating_pow(retry_attempt))).min(max_delay_ms);
                log::error!("[jetstream] Will retry connect in {}ms", delay);
                sleep(Duration::from_millis(delay)).await;
                retry_attempt = retry_attempt.saturating_add(1);
                continue;
            }
        }

        let jetstream_config = JetstreamConfig {
            endpoint: endpoints[endpoint_index].clone(),
            wanted_dids: did.clone(),
            compression: JetstreamCompression::Zstd,
            cursor,
            wanted_collections: nsid.clone(),
            max_retries: 0,
            ..Default::default()
        };

        let jetstream = match JetstreamConnector::new(jetstream_config) {
            Ok(j) => j,
            Err(e) => {
                log::error!("[jetstream] Failed to create Jetstream connector: {}", e);
                // Toggle endpoint and backoff
                endpoint_index = (endpoint_index + 1) % endpoints.len();
                log::warn!(
                    "[jetstream] Toggling endpoint due to connector error. Next endpoint: {}",
                    endpoints[endpoint_index]
                );
                let delay =
                    (base_delay_ms * (2_u64.saturating_pow(retry_attempt))).min(max_delay_ms);
                log::error!("[jetstream] Will retry creating connector in {}ms", delay);
                sleep(Duration::from_millis(delay)).await;
                retry_attempt = retry_attempt.saturating_add(1);
                continue;
            }
        };

        // Use existing connector API (which spawns the background websocket task) and receive a channel.
        let receiver = match jetstream.connect().await {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("[jetstream] Failed to connect to Jetstream: {}", e);
                endpoint_index = (endpoint_index + 1) % endpoints.len();
                log::warn!(
                    "[jetstream] Toggling endpoint due to connect error. Next endpoint: {}",
                    endpoints[endpoint_index]
                );
                let delay =
                    (base_delay_ms * (2_u64.saturating_pow(retry_attempt))).min(max_delay_ms);
                log::error!("[jetstream] Will retry connect in {}ms", delay);
                sleep(Duration::from_millis(delay)).await;
                retry_attempt = retry_attempt.saturating_add(1);
                continue;
            }
        };

        log::warn!(
            "[jetstream] Listening for: {} and emoji: {} on endpoint {}",
            listener_config.poster_did,
            listener_config.target_emoji,
            endpoints[endpoint_index]
        );

        // Track connection health to reset backoff on healthy sessions.
        let connected_at = Instant::now();
        let mut saw_event = false;

        // Consume events until the receiver is closed; then reconnect.
        // Note: JetstreamReceiver's `recv_async` is used by existing consumer code in this repo.
        while let Ok(event) = receiver.recv_async().await {
            saw_event = true;

            if let Commit(CommitEvent::Create { info, commit }) = event {
                if let AppBskyFeedPost(record) = commit.record {
                    // check and see if this post is what we're looking for
                    log::warn!("[jetstream] Checking record: {}", record.text);
                    if record.text.starts_with(&listener_config.target_emoji) {
                        if let Some(facets) = record.facets.clone() {
                            let features = facets
                                .iter()
                                .flat_map(|facet| &facet.features)
                                .filter_map(|feature| {
                                    if let atrium_api::types::Union::Refs(MainFeaturesItem::Link(
                                        link,
                                    )) = feature
                                    {
                                        Some((commit.info.rkey.clone(), link.uri.clone()))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();

                            for (rkey, uri) in features {
                                let Some(slug) = uri.split_terminator('/').next_back() else {
                                    log::error!(
                                        "[jetstream] Failed to extract slug from URI: {}",
                                        uri
                                    );
                                    continue;
                                };

                                let time_us = info.time_us.to_string();

                                if let Err(e) = sqlx::query("INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING")
                                    .bind(slug)
                                    .bind(&rkey)
                                    .bind(&time_us)
                                    .execute(&pool)
                                    .await
                                {
                                    log::error!("[jetstream] Failed to insert post: {}", e);
                                } else {
                                    log::warn!("[jetstream] Inserted post");
                                }
                            }
                        }
                    }
                }
            }
        }

        // Receiver closed: treat as disconnection and attempt reconnect.
        log::error!("[jetstream] Receiver closed; will attempt to reconnect");

        // Toggle endpoint on unexpected disconnect so next attempt tries the other instance.
        endpoint_index = (endpoint_index + 1) % endpoints.len();
        log::warn!(
            "[jetstream] Toggling endpoint due to disconnect. Next endpoint: {}",
            endpoints[endpoint_index]
        );

        // Reset retry_attempt if this connection was healthy (saw events or lasted past threshold).
        if saw_event || connected_at.elapsed().as_secs() > success_threshold_s {
            retry_attempt = 0;
            log::warn!("[jetstream] Connection was healthy; resetting retry attempts");
        } else {
            retry_attempt = retry_attempt.saturating_add(1);
        }

        // Backoff before reconnecting.
        let delay = (base_delay_ms * (2_u64.saturating_pow(retry_attempt))).min(max_delay_ms);
        log::error!(
            "[jetstream] Reconnecting in {}ms (attempt #{})",
            delay,
            retry_attempt
        );
        sleep(Duration::from_millis(delay)).await;

        // Outer loop continues and will attempt the next endpoint / reconnect.
    }
}
