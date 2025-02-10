#[macro_use]
extern crate rocket;

pub mod models;
mod post_listener;

use models::Meta;
use rocket::fairing::AdHoc;
use rocket::serde::json::Json;

use rocket_db_pools::sqlx::Row;
use rocket_db_pools::{sqlx, Connection, Database};

#[derive(Database)]
#[database("bluesky_comments")]
struct Comments(sqlx::PgPool);

#[get("/")]
fn index() -> &'static str {
    "at-comments database API server"
}

#[get("/<slug>")]
async fn post_meta(mut db: Connection<Comments>, slug: &str) -> Option<Json<Meta>> {
    sqlx::query("SELECT * FROM posts WHERE slug = $1")
        .bind(slug)
        .fetch_one(&mut **db)
        .await
        .map(|row| {
            let meta = Meta {
                id: row.get(0),
                slug: row.get(1),
                rkey: row.get(2),
                time_us: row.get(3),
            };
            Json(meta)
        })
        .ok()
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(Comments::init()) // init the database
        .attach(AdHoc::try_on_ignite("Jetstream listener", |rocket| async {
            let pool = match Comments::fetch(&rocket) {
                Some(pool) => pool.0.clone(), // clone the wrapped pool
                None => return Err(rocket),
            };
            rocket::tokio::task::spawn(post_listener::websocket_listener(pool)); // spawn jetstream listener, pass it a clone of the DB
            Ok(rocket)
        }))
        .mount("/", routes![index, post_meta])
}
