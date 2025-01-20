#[macro_use]
extern crate rocket;

pub mod models;
pub mod schema;
mod db;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[get("/slug/<id>")]
fn get_post(id: &str) -> String {

    let rkey = crate::db::get_post_rkey(id);
    format!("bsky rkey: {}", rkey)
}

#[launch]
async fn rocket() -> _ {
    // setup websocket stuff

    // setup server to respond
    rocket::build()
        .mount("/", routes![index])
        .mount("/", routes![get_post])
}
