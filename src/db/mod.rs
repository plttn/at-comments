
use diesel::prelude::*;
use dotenvy::dotenv;
use std::env;
use super::models::{Post, NewPost};

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}


pub fn create_post<'a>(conn: &mut PgConnection, slug: &'a str, rkey: &'a str) -> Post {
    use crate::schema::posts;

    let new_post = NewPost {
        slug,
        rkey,
    };

    diesel::insert_into(posts::table)
        .values(&new_post)
        .returning(Post::as_returning())
        .get_result(conn)
        .expect("Error saving new post")
}

pub fn get_post_rkey(post_slug: &str) -> String {
    use super::schema::posts::dsl::*;

    let connection = &mut establish_connection();
    let post = posts
        .filter(slug.eq(post_slug))
        .select(rkey)
        .first(connection)
        .expect("Error loading post");

    post
}