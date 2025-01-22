#[macro_use]
extern crate rocket;

use dotenvy::dotenv;
use jetstream::subscribe_posts;
use rocket::fairing::AdHoc;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket_db_pools::diesel::{prelude::*, PgPool};
use rocket_db_pools::{Connection, Database};
use std::env;

mod jetstream;

#[derive(Database)]
#[database("bluesky_comments")]
struct Db(PgPool);

#[derive(Queryable, Insertable, Serialize, Selectable)]
#[serde(crate = "rocket::serde")]
#[diesel(table_name = posts)]
pub struct Post {
    pub id: i32,
    pub slug: String,
    pub rkey: String,
    pub time_us: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = posts)]
pub struct NewPost<'a> {
    pub slug: &'a str,
    pub rkey: &'a str,
    pub time_us: &'a str,
}

diesel::table! {
    posts (id) {
        id -> Int4,
        slug -> Text,
        rkey -> Text,
        time_us -> Text,
    }
}

#[get("/meta/<input>")]
async fn meta(mut db: Connection<Db>, input: &str) -> Option<Json<Post>> {
    let post = posts::table
        .filter(posts::slug.eq(input))
        .first(&mut db)
        .await;
    post.ok().map(Json)
}

async fn latest_time_us(mut db: Connection<Db>) -> Result<String, diesel::result::Error> {
    posts::table
        .order(posts::id.desc())
        .select(posts::time_us)
        .first(&mut db)
        .await
}

async fn insert_post_meta(
    mut db: Connection<Db>,
    slug: &str,
    rkey: &str,
    time_us: &str,
) -> Result<Post, diesel::result::Error> {
    let new_post = NewPost {
        slug,
        rkey,
        time_us,
    };

    diesel::insert_into(posts::table)
        .values(&new_post)
        .on_conflict(posts::slug)
        .do_nothing()
        .returning(Post::as_returning())
        .get_result(&mut db)
        .await
}

#[launch]
fn rocket() -> _ {
    dotenv().ok();

    let figment = rocket::Config::figment().merge((
        "databases.bluesky_comments.url",
        env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
    ));
    rocket::custom(figment)
        .attach(Db::init())
        .attach(AdHoc::try_on_ignite("Websocket Listener", |rocket| async {
            if let Some(db) = Db::fetch(&rocket) {
                // run migrations using `db`. get the inner type with &db.0.
                rocket::tokio::task::spawn(jetstream::subscribe_posts(db));
                Ok(rocket)
            } else {
                Err(rocket)
            }
        }))
        .mount("/", routes![meta])
}
