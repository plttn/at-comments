pub mod models;
pub mod schema;

use diesel::prelude::*;
use dotenvy::dotenv;
use std::env;
use self::models::{Post, NewPost};

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}


pub fn create_post<'a>(conn: &mut PgConnection, slug: &'a str, post_did: &'a str) -> Post {
    use crate::schema::posts;

    let new_post = NewPost {
        slug,
        post_did,
    };

    diesel::insert_into(posts::table)
        .values(&new_post)
        .returning(Post::as_returning())
        .get_result(conn)
        .expect("Error saving new post")
}