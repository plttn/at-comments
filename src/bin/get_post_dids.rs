use self::models::*;
use at_comments::*;
use diesel::prelude::*;

fn main() {
    use self::schema::posts::dsl::*;

    let connection = &mut establish_connection();
    let results = posts
        .limit(5)
        .select(Post::as_select())
        .load(connection)
        .expect("Error loading posts");

    println!("Displaying {} posts", results.len());
    for post in results {
        println!("{:?}", post.slug);
        println!("{:?}", post.post_did);
    }
}
