use actix_web::{App, HttpServer, web};
use log::info;

mod image_processor;
mod routes;
mod database;
mod config;

use routes::setup_routes;
use database::Database;
use config::Config;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    
    // 설정 로드
    let config = match Config::new() {
        Ok(cfg) => {
            info!("✅ 설정 로드 성공");
            cfg
        }
        Err(e) => {
            eprintln!("❌ 설정 로드 실패: {}", e);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Config loading failed"));
        }
    };
    
    info!("🚀 BigPicture Backend 서버가 시작됩니다...");
    info!("📍 서버 주소: http://{}", config.server_address());
    
    // 데이터베이스 연결
    let database = match Database::new(&config).await {
        Ok(db) => {
            info!("✅ PostgreSQL 데이터베이스 연결 성공");
            db
        }
        Err(e) => {
            eprintln!("❌ 데이터베이스 연결 실패: {}", e);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Database connection failed"));
        }
    };
    
    let server_address = config.server_address();
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(database.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(setup_routes)
    })
    .bind(&server_address)?
    .run()
    .await
}
