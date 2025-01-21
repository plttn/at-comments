use super::models::{NewPost, Post};
use diesel::prelude::*;
use dotenvy::dotenv;
use std::env;

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}

pub fn insert_post_rkey<'a>(
    slug: &'a str,
    rkey: &'a str,
    time_us: &'a str,
) -> Result<Post, diesel::result::Error> {
    use crate::schema::posts;

    let new_post = NewPost {
        slug,
        rkey,
        time_us,
    };

    let connection = &mut establish_connection();

    diesel::insert_into(posts::table)
        .values(&new_post)
        .on_conflict(posts::slug) // if the slug already exists, do nothing
        .do_nothing()
        .returning(Post::as_returning())
        .get_result(connection)
}

pub fn post_meta(post_slug: &str) -> Result<Post, diesel::result::Error> {
    use super::schema::posts::dsl::*;

    let connection = &mut establish_connection();
    let post = posts.filter(slug.eq(post_slug)).first(connection);
    println!("{:?}", post);
    post
}

// get the latest time_us from the database for startup to backfill any missed posts
pub fn latest_time_us() -> Result<String, diesel::result::Error> {
    use super::schema::posts::dsl::*;

    let connection = &mut establish_connection();
    let time_stamp = posts.order(id.desc()).select(time_us).first(connection);

    time_stamp
}
