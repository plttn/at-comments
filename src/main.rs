#[macro_use]
extern crate rocket;

mod db;
pub mod models;
mod post_listener;
pub mod schema;

use rocket::tokio;
use rocket::serde::json::Json;
use rocket::response::status::NotFound;

#[get("/")]
fn index() -> &'static str {
    "at-comments database API server"
}

#[get("/meta/<slug>")]
fn get_post(slug: &str) -> Result<Json<models::Post>, NotFound<String>> {
    match db::get_post_meta(slug) {
        Ok(post) => Ok(Json(post)),
        Err(_) => Err(NotFound("Resource was not found.".to_string())),
    }
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
