use actix_web::{web, HttpResponse, Result};
use actix_multipart::Multipart;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use log::{info, error};
use std::time::Instant;

use crate::image_processor::ImageProcessor;
use crate::config::Config;
use crate::s3_service::S3Service;

#[derive(Serialize, Deserialize)]
pub struct S3ImageResponse {
    pub success: bool,
    pub message: String,
    pub filename: Option<String>,
    pub size_mb: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,
    pub s3_url: Option<String>,
}

// S3 업로드 내부 함수들
pub async fn upload_image_s3(
    mut payload: Multipart, 
    image_type: &str, 
    processor: ImageProcessor,
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    s3_service: web::Data<S3Service>
) -> Result<HttpResponse> {
    let start_time = Instant::now();
    info!("🚀 S3 업로드 시작...");
    
    let mut image_data = Vec::new();
    let mut filename = String::new();
    
    // 멀티파트 데이터 처리
    info!("📥 파일 데이터 수신 중...");
    while let Some(Ok(mut field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        
        if let Some(name) = content_disposition.get_name() {
            if name == "image" {
                if let Some(original_filename) = content_disposition.get_filename() {
                    filename = original_filename.to_string();
                    info!("📁 파일명: {}", filename);
                    
                    // 파일 형식 검증
                    if !processor.is_valid_image_format(&filename) {
                        return Ok(HttpResponse::BadRequest().json(S3ImageResponse {
                            success: false,
                            message: "지원되지 않는 이미지 형식입니다. (jpg, jpeg, png, gif, bmp, webp)".to_string(),
                            filename: None,
                            size_mb: None,
                            width: None,
                            height: None,
                            format: None,
                            s3_url: None,
                        }));
                    }
                }
                
                // 이미지 데이터 수집
                let mut chunk_count = 0;
                let mut last_log_time = Instant::now();
                while let Some(chunk) = field.next().await {
                    let data = chunk.map_err(|e| {
                        actix_web::error::ErrorInternalServerError(format!("파일 읽기 실패: {}", e))
                    })?;
                    image_data.extend_from_slice(&data);
                    chunk_count += 1;
                    
                    // 큰 파일(5MB 이상)인 경우에만 진행 상황 로그
                    let current_size_mb = image_data.len() as f64 / (1024.0 * 1024.0);
                    if current_size_mb > 5.0 {
                        let now = Instant::now();
                        if now.duration_since(last_log_time).as_secs() >= 1 {
                            info!("📦 청크 수신: {}개 (현재 크기: {:.2}MB)", 
                                  chunk_count, 
                                  current_size_mb);
                            last_log_time = now;
                        }
                    }
                }
                let final_size_mb = image_data.len() as f64 / (1024.0 * 1024.0);
                if final_size_mb > 1.0 {
                    info!("✅ 파일 데이터 수신 완료: {:.2}MB", final_size_mb);
                }
            }
        }
    }
    
    if image_data.is_empty() {
        return Ok(HttpResponse::BadRequest().json(S3ImageResponse {
            success: false,
            message: "이미지 파일이 필요합니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            s3_url: None,
        }));
    }
    
    // 파일 크기 검증
    let file_size_mb = processor.get_file_size_mb(&image_data);
    let max_size_mb = config.max_file_size_mb;
    info!("📊 파일 크기: {:.2}MB, 제한: {:.2}MB", file_size_mb, max_size_mb);
    
    if file_size_mb > max_size_mb {
        return Ok(HttpResponse::BadRequest().json(S3ImageResponse {
            success: false,
            message: format!("파일 크기는 {:.0}MB를 초과할 수 없습니다 (현재: {:.2}MB)", max_size_mb, file_size_mb),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            s3_url: None,
        }));
    }
    
    // 이미지 처리 (리사이즈 + WebP 변환)
    let file_size_mb = processor.get_file_size_mb(&image_data);
    if file_size_mb > 1.0 {
        info!("🖼️ 이미지 처리 시작 (리사이즈 + WebP 변환)...");
    }
    let process_start = Instant::now();
    let processed_data = match processor.process_image(&image_data) {
        Ok(data) => {
            let process_time = process_start.elapsed();
            if file_size_mb > 1.0 {
                info!("✅ 이미지 처리 완료: {:.2}초 (처리된 크기: {:.2}MB)", 
                      process_time.as_secs_f64(), 
                      data.len() as f64 / (1024.0 * 1024.0));
            }
            data
        },
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(S3ImageResponse {
                success: false,
                message: format!("이미지 처리 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                s3_url: None,
            }));
        }
    };
    
    // S3 업로드
    info!("☁️ S3 업로드 시작...");
    let upload_start = Instant::now();
    let s3_url = match s3_service.upload_thumbnail(processed_data, &filename).await {
        Ok(url) => {
            let upload_time = upload_start.elapsed();
            info!("✅ S3 업로드 완료: {:.2}초", upload_time.as_secs_f64());
            url
        },
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(S3ImageResponse {
                success: false,
                message: format!("S3 업로드 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                s3_url: None,
            }));
        }
    };
    
    // 이미지 정보 가져오기
    let (width, height, format) = match processor.get_image_info(&image_data) {
        Ok(info) => (Some(info.0), Some(info.1), info.2),
        Err(_) => (None, None, "Unknown".to_string()),
    };
    
    let file_size_mb = processor.get_file_size_mb(&image_data);
    let total_time = start_time.elapsed();
    info!("🎉 전체 업로드 완료: {:.2}초", total_time.as_secs_f64());
    
    Ok(HttpResponse::Ok().json(S3ImageResponse {
        success: true,
        message: "S3 업로드 성공".to_string(),
        filename: Some(filename),
        size_mb: Some(file_size_mb),
        width,
        height,
        format: Some(format),
        s3_url: Some(s3_url),
    }))
}

pub async fn upload_circular_thumbnail_s3_internal(
    mut payload: Multipart, 
    image_type: &str, 
    processor: ImageProcessor,
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    s3_service: web::Data<S3Service>
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
                        return Ok(HttpResponse::BadRequest().json(S3ImageResponse {
                            success: false,
                            message: "지원되지 않는 이미지 형식입니다. (jpg, jpeg, png, gif, bmp, webp)".to_string(),
                            filename: None,
                            size_mb: None,
                            width: None,
                            height: None,
                            format: None,
                            s3_url: None,
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
        return Ok(HttpResponse::BadRequest().json(S3ImageResponse {
            success: false,
            message: "이미지 파일이 필요합니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            s3_url: None,
        }));
    }
    
    // 파일 크기 검증
    let file_size_mb = processor.get_file_size_mb(&image_data);
    let max_size_mb = config.max_file_size_mb;
    info!("📊 파일 크기: {:.2}MB, 제한: {:.2}MB", file_size_mb, max_size_mb);
    
    if file_size_mb > max_size_mb {
        return Ok(HttpResponse::BadRequest().json(S3ImageResponse {
            success: false,
            message: format!("파일 크기는 {:.0}MB를 초과할 수 없습니다 (현재: {:.2}MB)", max_size_mb, file_size_mb),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            s3_url: None,
        }));
    }
    
    // 원형 썸네일 처리 (크롭 + 원형 마스킹 + WebP 변환)
    let processed_data = match processor.process_circular_thumbnail(&image_data) {
        Ok(data) => data,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(S3ImageResponse {
                success: false,
                message: format!("이미지 처리 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                s3_url: None,
            }));
        }
    };
    
    // S3 업로드
    let s3_url = match s3_service.upload_circular_thumbnail(processed_data, &filename).await {
        Ok(url) => url,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(S3ImageResponse {
                success: false,
                message: format!("S3 업로드 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                s3_url: None,
            }));
        }
    };
    
    // 이미지 정보 가져오기
    let (width, height, format) = match processor.get_image_info(&image_data) {
        Ok(info) => (Some(info.0), Some(info.1), info.2),
        Err(_) => (None, None, "Unknown".to_string()),
    };
    
    let file_size_mb = processor.get_file_size_mb(&image_data);
    
    Ok(HttpResponse::Ok().json(S3ImageResponse {
        success: true,
        message: "S3 원형 썸네일 업로드 성공".to_string(),
        filename: Some(filename),
        size_mb: Some(file_size_mb),
        width,
        height,
        format: Some(format),
        s3_url: Some(s3_url),
    }))
} 