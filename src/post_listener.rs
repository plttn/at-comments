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

    let cursor = match sqlx::query("SELECT time_us FROM posts ORDER BY id DESC LIMIT 1")
        .fetch_one(&pool)
        .await
    {
        Ok(row) => {
            let time = row.get::<String, _>(0);
            chrono::DateTime::from_timestamp_micros(time.parse::<i64>().unwrap())
        }
        Err(_) => None,
    };

    let jetstream_config = JetstreamConfig {
        endpoint: DefaultJetstreamEndpoints::USEastOne.into(),
        wanted_dids: did,
        compression: JetstreamCompression::Zstd,
        cursor,
        wanted_collections: nsid,
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
                    let facets = record.facets.clone().unwrap();
                    for facet in facets {
                        for feature in &facet.features {
                            if let atrium_api::types::Union::Refs(MainFeaturesItem::Link(link)) =
                                feature
                            {
                                let rkey = commit.info.rkey.clone();
                                let uri = link.uri.clone();
                                // get the slug
                                let uri_parts: Vec<&str> = uri.split('/').collect();
                                let slug = match uri_parts.last() {
                                    Some(&last_part) => last_part,
                                    None => {
                                        log::error!(
                                            "[jetstream] Failed to extract slug from URI: {}",
                                            uri
                                        );
                                        continue;
                                    }
                                };
                                let time_us_string = info.time_us.to_string();
                                let time_us = &time_us_string;

                                match sqlx::query("INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING")
                                        .bind(slug)
                                        .bind(rkey)
                                        .bind(time_us)
                                        .execute(& pool)
                                        .await {
                                            Ok(_) => {
                                                log::info!("[jetstream] Inserted post");
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "[jetstream] Failed to insert post: {}",
                                                    e
                                                );
                                            }
                                        }
                            }
                        }
                    }
                }
            }
        }
    }
}
