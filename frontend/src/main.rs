use std::{env, path::PathBuf};

use actix_files::Files;
use actix_web::{get, App, HttpResponse, HttpServer, Responder};

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body(include_str!("../static/index.html"))
}

#[get("/login")]
async fn login() -> impl Responder {
    HttpResponse::Ok().body(include_str!("../static/login.html"))
}

#[get("/register")]
async fn register() -> impl Responder {
    HttpResponse::Ok().body(include_str!("../static/register.html"))
}

// so actix_files has this lovely behaviour where relative paths
// are relative to the directory I run cargo from
fn find_project_root() -> Option<PathBuf> {
    let mut current_dir = env::current_dir().expect("Failed to read current directory.");

    loop {
        if current_dir.join("Cargo.lock").exists() {
            return Some(current_dir);
        }

        if !current_dir.pop() {
            return None;
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let project_root = find_project_root().expect("Expected to be in a cargo project.");
    let static_folder = project_root.join("frontend").join("static");
    HttpServer::new(move || {
        App::new()
            .service(index)
            .service(login)
            .service(register)
            .service(Files::new("/static", static_folder.clone()))
    })
    .bind("127.0.0.1:3030")?
    .run()
    .await
}
