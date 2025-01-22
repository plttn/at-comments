use atrium_api::app::bsky::richtext::facet::MainFeaturesItem;
use atrium_api::record::KnownRecord::AppBskyFeedPost;
// use chrono;
use crate::{latest_time_us, Db};
use jetstream_oxide::{
    events::{commit::CommitEvent, JetstreamEvent::Commit},
    exports::{Did, Nsid},
    DefaultJetstreamEndpoints, JetstreamCompression, JetstreamConfig, JetstreamConnector,
};
use rocket_db_pools::Connection;
use std::env;

pub async fn subscribe_posts(mut db: Connection<Db>) {
    let did_string = env::var("POSTER_DID").expect("POSTER_DID must be set");
    let target_emoji = env::var("TARGET_EMOJI").expect("TARGET_EMOJI must be set");
    let nsid = vec![Nsid::new("app.bsky.feed.post".to_string()).unwrap()];

    let did = vec![Did::new(did_string.to_string()).unwrap()];
    let cursor = match latest_time_us(db).await {
        Ok(time) => chrono::DateTime::from_timestamp_micros(time.parse::<i64>().unwrap()),
        Err(_) => None,
    };

    let config = JetstreamConfig {
        endpoint: DefaultJetstreamEndpoints::USWestOne.into(),
        wanted_dids: did,
        compression: JetstreamCompression::Zstd,
        cursor,
        wanted_collections: nsid,
    };

    let jetstream = match JetstreamConnector::new(config) {
        Ok(jetstream) => jetstream,
        Err(e) => {
            eprintln!("Failed to create Jetstream connector: {}", e);
            return;
        }
    };

    let (receiver, _) = match jetstream.connect().await {
        Ok(connection) => connection,
        Err(e) => {
            eprintln!("Failed to connect to Jetstream: {}", e);
            return;
        }
    };

    println!("Connected to Jetstream");
    while let Ok(event) = receiver.recv_async().await {
        let Commit(CommitEvent::Create { info, commit }) = event else {
            continue;
        };

        // if let AppBskyFeedPost(record) != commit.record {
        //     continue;
        // }

        if let AppBskyFeedPost(record) = commit.record {
            if !record.text.starts_with(&target_emoji) {
                continue;
            }

            record
                .facets
                .into_iter()
                .flatten()
                .flat_map(move |facet| facet.features)
                .filter_map(|feature| match feature {
                    atrium_api::types::Union::Refs(MainFeaturesItem::Link(link)) => Some(link),
                    _ => None,
                })
                .for_each(|link| {
                    println!("Link: {}", link.uri);
                    let rkey = commit.info.rkey.clone();
                    let uri = link.uri.clone();
                    // get the slug
                    let uri_parts: Vec<&str> = uri.split('/').collect();
                    let slug = *uri_parts.last().unwrap();
                    let time_us_string = info.time_us.to_string();
                    let time_us = time_us_string.as_str();

                    // insert into db
                    // should probably insert cursor too
                    let _ = insert_post_rkey(slug, &rkey, time_us);
                })
        }
    }
}
