use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct Meta {
    pub slug: String,
    pub rkey: String,
    pub time_us: i64,
}
