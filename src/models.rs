use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct Meta {
    pub id: i32,
    pub slug: String,
    pub rkey: String,
    pub time_us: String,
}
