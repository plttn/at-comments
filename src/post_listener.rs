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

#[derive(Deserialize, Debug)]
struct ListenerConfig {
    poster_did: String,
    target_emoji: String,
}

pub async fn websocket_listener(pool: sqlx::Pool<sqlx::Postgres>) {
    let listener_config = Config::figment().extract::<ListenerConfig>().unwrap();
    let nsid = vec![Nsid::new("app.bsky.feed.post".to_string()).unwrap()];

    let did = vec![Did::new(listener_config.poster_did.clone()).unwrap()];

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

    let diff = match cursor {
        Some(cursor) => chrono::Utc::now() - cursor,
        None => chrono::Duration::days(1),
    };

    if diff.num_days() >= 1 {
        log::warn!("[jetstream] Cursor is more than a day old, resetting cursor");
        cursor = None;
    }

    let jetstream_config = JetstreamConfig {
        endpoint: DefaultJetstreamEndpoints::USWestOne.into(),
        wanted_dids: did,
        compression: JetstreamCompression::Zstd,
        cursor,
        wanted_collections: nsid,
        ..Default::default()
    };

    let jetstream = match JetstreamConnector::new(jetstream_config) {
        Ok(jetstream) => jetstream,
        Err(e) => {
            log::error!("[jetstream] Failed to create Jetstream connector: {}", e);
            return;
        }
    };

    let receiver = match jetstream.connect().await {
        Ok(connection) => connection,
        Err(e) => {
            log::error!("[jetstream] Failed to connect to Jetstream: {}", e);
            return;
        }
    };

    log::warn!(
        "[jetstream] Listening for: {} and emoji: {}",
        listener_config.poster_did,
        listener_config.target_emoji
    );
    while let Ok(event) = receiver.recv_async().await {
        log::info!("[jetstream] received event");
        if let Commit(CommitEvent::Create { info, commit }) = event {
            if let AppBskyFeedPost(record) = commit.record {
                // check and see if this post is what we're looking for
                log::info!("[jetstream] Checking record: {}", record.text);
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
                                log::error!("[jetstream] Failed to extract slug from URI: {}", uri);
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
                                log::info!("[jetstream] Inserted post");
                            }
                        }
                    }
                }
            }
        }
    }
    log::error!("[jetstream] Post listener exited");
    std::process::exit(1);
}
