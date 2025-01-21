use crate::db::{insert_post_rkey, latest_time_us};
use atrium_api::app::bsky::richtext::facet::MainFeaturesItem;
use atrium_api::record::KnownRecord::AppBskyFeedPost;
use chrono;
use dotenvy::dotenv;
use jetstream_oxide::{
    events::{commit::CommitEvent, JetstreamEvent::Commit},
    exports::{Did, Nsid},
    DefaultJetstreamEndpoints, JetstreamCompression, JetstreamConfig, JetstreamConnector,
};
use std::env;

pub async fn subscribe_posts() {
    dotenv().ok();
    let did_string = env::var("POSTER_DID").expect("POSTER_DID must be set");
    let target_emoji = env::var("TARGET_EMOJI").expect("TARGET_EMOJI must be set");
    let nsid = vec![Nsid::new("app.bsky.feed.post".to_string()).unwrap()];

    let did = vec![Did::new(did_string.to_string()).unwrap()];

    let cursor = match latest_time_us() {
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

    let jetstream = JetstreamConnector::new(config).unwrap();

    let (receiver, _) = jetstream.connect().await.unwrap();

    while let Ok(event) = receiver.recv_async().await {
        if let Commit(commit) = event {
            match commit {
                CommitEvent::Create { info, commit } => {
                    if let AppBskyFeedPost(record) = commit.record {
                        // check and see if this post is what we're looking for

                        if record.text.starts_with(target_emoji.as_str()) {
                            let facets = record.facets.clone().unwrap();
                            for facet in facets {
                                for feature in &facet.features {
                                    match feature {
                                        atrium_api::types::Union::Refs(MainFeaturesItem::Link(
                                            link,
                                        )) => {
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
                                        }
                                        _ => {} // ick
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
