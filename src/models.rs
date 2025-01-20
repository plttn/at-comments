use diesel::prelude::*;
use rocket::serde::Serialize;
use crate::schema::posts;

#[derive(Queryable, Selectable, Serialize)]
#[serde(crate = "rocket::serde")]
#[diesel(table_name = crate::schema::posts)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Post {
    pub id: i32,
    pub slug: String,
    pub rkey: String,
}

#[derive(Insertable)]
#[diesel(table_name = posts)]
pub struct NewPost<'a> {
    pub slug: &'a str,
    pub rkey: &'a str,
}