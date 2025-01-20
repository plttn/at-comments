#[macro_use]
extern crate rocket;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[get("/slug/<id>")]
fn get_post(id: &str) -> String {
    format!("Post ID: {}", id)
}

#[launch]
async fn rocket() -> _ {
    // setup websocket stuff

    // setup server to respond
    rocket::build()
        .mount("/", routes![index])
        .mount("/", routes![get_post])
}
