use actix_web::{web, HttpResponse, Result};
use actix_multipart::Multipart;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;
use chrono::Utc;
use std::fs;

use crate::image_processor::{ImageProcessor, create_thumbnail_processor, create_map_processor};

#[derive(Serialize, Deserialize)]
pub struct ImageResponse {
    pub success: bool,
    pub message: String,
    pub filename: Option<String>,
    pub size_mb: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,
    pub url: Option<String>,
}

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config
        .service(
            web::scope("/api")
                .route("/health", web::get().to(health_check))
                .service(
                    web::scope("/images")
                        .route("/upload/thumbnail", web::post().to(upload_thumbnail))
                        .route("/upload/map", web::post().to(upload_map_image))
                        .route("/info/{filename:.*}", web::get().to(get_image_info))
                        .route("/download/{filename:.*}", web::get().to(download_image))
                )
        )
        .route("/", web::get().to(index));
}

async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "BigPicture Backend API",
        "status": "running"
    })))
}

async fn health_check() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "bigpicture-backend"
    })))
}

async fn upload_thumbnail(mut payload: Multipart) -> Result<HttpResponse> {
    upload_image(payload, "thumbnail", create_thumbnail_processor()).await
}

async fn upload_map_image(mut payload: Multipart) -> Result<HttpResponse> {
    upload_image(payload, "map", create_map_processor()).await
}

async fn upload_image(
    mut payload: Multipart, 
    image_type: &str, 
    processor: ImageProcessor
) -> Result<HttpResponse> {
    let mut image_data = Vec::new();
    let mut filename = String::new();
    
    // 멀티파트 데이터 처리
    while let Some(Ok(mut field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        
        if let Some(name) = content_disposition.get_name() {
            if name == "image" {
                if let Some(original_filename) = content_disposition.get_filename() {
                    filename = original_filename.to_string();
                    
                    // 파일 형식 검증
                    if !processor.is_valid_image_format(&filename) {
                        return Ok(HttpResponse::BadRequest().json(ImageResponse {
                            success: false,
                            message: "지원되지 않는 이미지 형식입니다. (jpg, jpeg, png, gif, bmp, webp)".to_string(),
                            filename: None,
                            size_mb: None,
                            width: None,
                            height: None,
                            format: None,
                            url: None,
                        }));
                    }
                }
                
                // 이미지 데이터 수집
                while let Some(chunk) = field.next().await {
                    let data = chunk.map_err(|e| {
                        actix_web::error::ErrorInternalServerError(format!("파일 읽기 실패: {}", e))
                    })?;
                    image_data.extend_from_slice(&data);
                }
            }
        }
    }
    
    if image_data.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ImageResponse {
            success: false,
            message: "이미지 파일이 필요합니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 파일 크기 검증 (10MB 제한)
    if processor.get_file_size_mb(&image_data) > 10.0 {
        return Ok(HttpResponse::BadRequest().json(ImageResponse {
            success: false,
            message: "파일 크기는 10MB를 초과할 수 없습니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 이미지 처리 (WebP 변환)
    let processed_data = match processor.process_image(&image_data) {
        Ok(data) => data,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ImageResponse {
                success: false,
                message: format!("이미지 처리 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                url: None,
            }));
        }
    };
    
    // 고유한 파일명 생성
    let timestamp = Utc::now().timestamp();
    let uuid = Uuid::new_v4().to_string()[..8].to_string();
    let webp_filename = format!("{}_{}_{}.webp", image_type, uuid, timestamp);
    
    // 업로드 디렉토리 생성
    let upload_dir = format!("./uploads/{}", image_type);
    if let Err(e) = fs::create_dir_all(&upload_dir) {
        return Ok(HttpResponse::InternalServerError().json(ImageResponse {
            success: false,
            message: format!("디렉토리 생성 실패: {}", e),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 파일 저장
    let filepath = format!("{}/{}", upload_dir, webp_filename);
    if let Err(e) = fs::write(&filepath, &processed_data) {
        return Ok(HttpResponse::InternalServerError().json(ImageResponse {
            success: false,
            message: format!("파일 저장 실패: {}", e),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 이미지 정보 가져오기
    let (width, height, format) = match processor.get_image_info(&processed_data) {
        Ok(info) => info,
        Err(_) => (0, 0, "WebP".to_string()),
    };
    
    let size = processor.get_file_size_mb(&processed_data);
    
    Ok(HttpResponse::Ok().json(ImageResponse {
        success: true,
        message: "이미지 업로드 성공".to_string(),
        filename: Some(webp_filename.clone()),
        size_mb: Some(size),
        width: Some(width),
        height: Some(height),
        format: Some(format),
        url: Some(format!("/api/images/download/{}", webp_filename)),
    }))
}

async fn get_image_info(path: web::Path<String>) -> Result<HttpResponse> {
    let filename = path.into_inner();
    
    // 파일 경로 찾기
    let filepath = find_image_file(&filename);
    if filepath.is_empty() {
        return Ok(HttpResponse::NotFound().json(ImageResponse {
            success: false,
            message: "파일을 찾을 수 없습니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 파일 읽기
    let file_data = match fs::read(&filepath) {
        Ok(data) => data,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ImageResponse {
                success: false,
                message: format!("파일 읽기 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                url: None,
            }));
        }
    };
    
    let processor = ImageProcessor::new(0, 0, 0); // 임시로 사용
    let (width, height, format) = match processor.get_image_info(&file_data) {
        Ok(info) => info,
        Err(_) => (0, 0, "WebP".to_string()),
    };
    
    let size = processor.get_file_size_mb(&file_data);
    
    Ok(HttpResponse::Ok().json(ImageResponse {
        success: true,
        message: "이미지 정보 조회 성공".to_string(),
        filename: Some(filename.clone()),
        size_mb: Some(size),
        width: Some(width),
        height: Some(height),
        format: Some(format),
        url: Some(format!("/api/images/download/{}", filename)),
    }))
}

async fn download_image(path: web::Path<String>) -> Result<HttpResponse> {
    let filename = path.into_inner();
    
    // 파일 경로 찾기
    let filepath = find_image_file(&filename);
    if filepath.is_empty() {
        return Ok(HttpResponse::NotFound().json(ImageResponse {
            success: false,
            message: "파일을 찾을 수 없습니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 파일 읽기
    let file_data = match fs::read(&filepath) {
        Ok(data) => data,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ImageResponse {
                success: false,
                message: format!("파일 읽기 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                url: None,
            }));
        }
    };
    
    Ok(HttpResponse::Ok()
        .content_type("image/webp")
        .body(file_data))
}

fn find_image_file(filename: &str) -> String {
    // 썸네일 디렉토리에서 검색
    let thumbnail_path = format!("./uploads/thumbnail/{}", filename);
    if Path::new(&thumbnail_path).exists() {
        return thumbnail_path;
    }
    
    // 지도 디렉토리에서 검색
    let map_path = format!("./uploads/map/{}", filename);
    if Path::new(&map_path).exists() {
        return map_path;
    }
    
    String::new()
} 