#[macro_use]
extern crate rocket;

mod db;
pub mod models;
mod post_listener;
pub mod schema;

use rocket::tokio;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[get("/slug/<slug>")]
fn get_post(slug: &str) -> Option<String> {
    let result = db::get_post_rkey(slug).ok();

    result
}

#[launch]
async fn rocket() -> _ {
    // setup websocket stuff
    tokio::spawn(post_listener::subscribe_posts());
    // setup server to respond
    rocket::build()
        .mount("/", routes![index])
        .mount("/", routes![get_post])
}
