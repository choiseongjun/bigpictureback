use actix_web::{App, HttpServer, web};
use actix_cors::Cors;
use log::info;
use http;

mod image_processor;
mod routes;
mod database;
mod config;
mod s3_service;
mod s3_routes;
mod error_handler;
mod emotions;

use routes::setup_routes;
use database::Database;
use config::Config;
use s3_service::S3Service;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // SQL 로깅을 위한 환경 변수 설정
    unsafe {
        std::env::set_var("RUST_LOG", "info,sqlx::query=debug");
    }
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
    
    // S3 서비스 초기화
    let s3_service = match S3Service::new(
        config.s3_bucket_name.clone(), 
        config.s3_region.clone(),
        config.s3_access_key_id.clone(),
        config.s3_secret_access_key.clone()
    ).await {
        Ok(s3) => {
            info!("✅ S3 서비스 초기화 성공");
            s3
        }
        Err(e) => {
            eprintln!("❌ S3 서비스 초기화 실패: {}", e);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "S3 service initialization failed"));
        }
    };
    
    let _server_address = config.server_address();
    HttpServer::new(move || {
        // CORS 설정 - 모든 origin 허용 (localhost, IP 주소, 도메인 모두)
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);
        
        App::new()
            .wrap(cors)
            .app_data(web::Data::new(database.pool.clone()))
            .app_data(web::Data::new(database.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(s3_service.clone()))
            .configure(setup_routes)
    })
    .bind("0.0.0.0:5500")?  // 모든 IP에서 접근 가능하도록 0.0.0.0으로 바인딩
    .run()
    .await
}
