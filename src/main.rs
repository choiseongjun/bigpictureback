use actix_web::{App, HttpServer};
use log::info;

mod image_processor;
mod routes;

use routes::setup_routes;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    
    info!("🚀 BigPicture Backend 서버가 시작됩니다...");
    info!("📍 서버 주소: http://localhost:5500");
    
    HttpServer::new(|| {
        App::new()
            .configure(setup_routes)
    })
    .bind("127.0.0.1:5500")?
    .run()
    .await
}
