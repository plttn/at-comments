use at_comments::*;

use std::io::stdin;

fn main() {
    let connection = &mut establish_connection();
    let mut slug = String::new();
    let mut post_did = String::new();

    println!("What would you like your slug to be?");
    stdin().read_line(&mut slug).unwrap();
    let slug = slug.trim_end(); // Remove the trailing newline

    println!("\n what's the post DID for {slug}? (Press {EOF} when finished)\n",);
    stdin().read_line(&mut post_did).unwrap();
    let post_did = post_did.trim_end(); // Remove the trailing newline

    let post = create_post(connection, slug, &post_did);
    println!("\nSaved post did for {slug} with id {}", post.id);
    
}

#[cfg(not(windows))]
const EOF: &str = "CTRL+D";

#[cfg(windows)]
const EOF: &str = "CTRL+Z";