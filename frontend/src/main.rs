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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(index).service(login).service(register))
        .bind("127.0.0.1:3030")?
        .run()
        .await
}
