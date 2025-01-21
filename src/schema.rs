// @generated automatically by Diesel CLI.

diesel::table! {
    posts (id) {
        id -> Int4,
        slug -> Text,
        rkey -> Text,
        time_us -> Text,
    }
}
