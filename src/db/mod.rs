use super::models::{NewPost, Post};
use diesel::prelude::*;
use dotenvy::dotenv;
use std::env;

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}

pub fn insert_post_rkey<'a>(slug: &'a str, rkey: &'a str) -> Post {
    use crate::schema::posts;

    let new_post = NewPost { slug, rkey };

    let connection = &mut establish_connection();

    diesel::insert_into(posts::table)
        .values(&new_post)
        .returning(Post::as_returning())
        .get_result(connection)
        .expect("Error saving new post")
}

pub fn get_post_rkey(post_slug: &str) -> Result<String, diesel::result::Error> {
    use super::schema::posts::dsl::*;

    let connection = &mut establish_connection();
    let post = posts
        .filter(slug.eq(post_slug))
        .select(rkey)
        .first(connection);

    post
}
