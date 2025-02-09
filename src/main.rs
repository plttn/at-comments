#[macro_use]
extern crate rocket;

pub mod models;
mod post_listener;
pub mod schema;

use models::Meta;
use rocket::fairing::AdHoc;
use rocket::response::status::NotFound;
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
async fn post_meta(
    mut db: Connection<Comments>,
    slug: &str,
) -> Result<Json<Meta>, NotFound<String>> {
    let result = sqlx::query("SELECT * FROM posts WHERE slug = $1")
        .bind(slug)
        .fetch_one(&mut **db)
        .await;

    match result {
        Ok(row) => {
            let meta = Meta {
                id: row.get(0),
                slug: row.get(1),
                rkey: row.get(2),
                time_us: row.get(3),
            };
            Ok(Json(meta))
        }
        Err(_) => Err(NotFound("Resource was not found.".to_string())),
    }
}

#[launch]
fn rocket() -> _ {
    // setup websocket stuff
    // tokio::spawn(post_listener::subscribe_posts());
    // setup server to respond
    rocket::build()
        .attach(Comments::init())
        .attach(AdHoc::try_on_ignite("Jetstream listener", |rocket| async {
            let pool = match Comments::fetch(&rocket) {
                Some(pool) => pool.0.clone(), // clone the wrapped pool
                None => return Err(rocket),
            };

            rocket::tokio::task::spawn(post_listener::websocket_listener(pool));

            Ok(rocket)
        }))
        .mount("/", routes![index, post_meta])
}
