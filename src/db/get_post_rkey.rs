use crate::models::*;
use at_comments::*;
use diesel::prelude::*;



fn get_post_rkey(slug: &str) -> String {
    use self::schema::posts::dsl::*;

    let connection = &mut establish_connection();
    let post = posts
        .filter(slug.eq(slug))
        .select(rkey)
        .first(connection)
        .expect("Error loading post");

    post
}