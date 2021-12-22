use rocket::{launch, routes, get};
use rocket::serde::{json::Json};
use serde::{Serialize};
use chrono::prelude::*;

#[get("/wu")]
fn watch_update() -> String {
    let now = Local::now();
    now.format("%Y%m%d%H%M%S").to_string()
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![index, watch_update])
}
