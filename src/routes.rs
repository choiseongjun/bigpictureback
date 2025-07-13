use actix_web::{web, HttpResponse, Result};
use actix_multipart::Multipart;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;
use chrono::Utc;
use std::fs;
use sqlx::PgPool;
use log::{info, warn, error};

use crate::image_processor::ImageProcessor;
use crate::database;
use crate::config::Config;
use crate::s3_service::S3Service;
use crate::s3_routes::{upload_image_s3, upload_circular_thumbnail_s3_internal};

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
                .route("/markers", web::get().to(get_markers))
                .service(
                    web::scope("/images")
                        .route("/upload/thumbnail", web::post().to(upload_thumbnail))
                        .route("/upload/map", web::post().to(upload_map_image))
                        .route("/generate/thumbnail", web::post().to(generate_thumbnail))
                        .route("/info/{filename:.*}", web::get().to(get_image_info))
                        .route("/download/{filename:.*}", web::get().to(download_image))
                        .route("/download/original/{filename:.*}", web::get().to(download_original_image))
                        .route("/list", web::get().to(list_images))
                        .route("/stats", web::get().to(get_image_stats))
                )
                .service(
                    web::scope("/s3")
                        .route("/upload/thumbnail", web::post().to(upload_thumbnail_s3))
                        .route("/upload/map", web::post().to(upload_map_s3))
                        .route("/upload/circular", web::post().to(upload_circular_thumbnail_s3))
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

#[derive(Deserialize)]
pub struct MarkersQuery {
    lat: f64,
    lng: f64,
    lat_delta: f64,
    lng_delta: f64,
    zoom: Option<i32>,
    emotion_tags: Option<String>,
    min_likes: Option<i32>,
    min_views: Option<i32>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    limit: Option<i32>,
}

async fn get_markers(
    query: web::Query<MarkersQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    info!("🔍 마커 조회 요청 받음:");
    info!("   - lat: {}", query.lat);
    info!("   - lng: {}", query.lng);
    info!("   - lat_delta: {}", query.lat_delta);
    info!("   - lng_delta: {}", query.lng_delta);
    info!("   - zoom: {:?}", query.zoom);
    info!("   - emotion_tags: {:?}", query.emotion_tags);
    info!("   - min_likes: {:?}", query.min_likes);
    info!("   - min_views: {:?}", query.min_views);
    info!("   - sort_by: {:?}", query.sort_by);
    info!("   - sort_order: {:?}", query.sort_order);
    info!("   - limit: {:?}", query.limit);
    
    let db = database::Database { pool: pool.get_ref().clone() };
    
    // 감성 태그 파싱
    let emotion_tags = query.emotion_tags.as_ref().map(|tags| {
        let parsed_tags: Vec<String> = tags.split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        info!("   - 파싱된 감성 태그: {:?}", parsed_tags);
        parsed_tags
    });
    
    // 정렬 순서 검증
    let sort_by = query.sort_by.as_deref();
    let sort_order = query.sort_order.as_deref();
    
    info!("   - 최종 정렬: {} {}", sort_by.unwrap_or("created_at"), sort_order.unwrap_or("desc"));
    
    match db.get_markers(
        query.lat,
        query.lng,
        query.lat_delta,
        query.lng_delta,
        emotion_tags,
        query.min_likes,
        query.min_views,
        sort_by,
        sort_order,
        query.limit,
    ).await {
        Ok(markers) => {
            info!("✅ 마커 조회 성공: {}개 마커 반환", markers.len());
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": markers,
                "count": markers.len()
            })))
        }
        Err(e) => {
            error!("❌ 마커 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("마커 조회 실패: {}", e)
            })))
        }
    }
}

// S3 업로드 함수들
async fn upload_thumbnail_s3(
    payload: Multipart, 
    pool: web::Data<PgPool>, 
    config: web::Data<Config>,
    s3_service: web::Data<S3Service>
) -> Result<HttpResponse> {
    let processor = ImageProcessor::new(
        config.thumbnail_max_width,
        config.thumbnail_max_height,
        config.thumbnail_quality
    );
    upload_image_s3(payload, "thumbnail", processor, pool, config, s3_service).await
}

async fn upload_map_s3(
    payload: Multipart, 
    pool: web::Data<PgPool>, 
    config: web::Data<Config>,
    s3_service: web::Data<S3Service>
) -> Result<HttpResponse> {
    let processor = ImageProcessor::new(
        config.map_max_width,
        config.map_max_height,
        config.map_quality
    );
    upload_image_s3(payload, "map", processor, pool, config, s3_service).await
}

async fn upload_circular_thumbnail_s3(
    payload: Multipart, 
    pool: web::Data<PgPool>, 
    config: web::Data<Config>,
    s3_service: web::Data<S3Service>
) -> Result<HttpResponse> {
    let processor = ImageProcessor::new(250, 250, 85);
    upload_circular_thumbnail_s3_internal(payload, "circular_thumbnail", processor, pool, config, s3_service).await
}

async fn upload_thumbnail(payload: Multipart, pool: web::Data<PgPool>, config: web::Data<Config>) -> Result<HttpResponse> {
    let processor = ImageProcessor::new(
        config.thumbnail_max_width,
        config.thumbnail_max_height,
        config.thumbnail_quality
    );
    upload_image(payload, "thumbnail", processor, pool, config).await
}

async fn upload_map_image(payload: Multipart, pool: web::Data<PgPool>, config: web::Data<Config>) -> Result<HttpResponse> {
    let processor = ImageProcessor::new(
        config.map_max_width,
        config.map_max_height,
        config.map_quality
    );
    upload_image(payload, "map", processor, pool, config).await
}

async fn generate_thumbnail(payload: Multipart, pool: web::Data<PgPool>, config: web::Data<Config>) -> Result<HttpResponse> {
    // 250x250 원형 썸네일용 프로세서 생성
    let processor = ImageProcessor::new(250, 250, 85);
    upload_circular_thumbnail(payload, "generated_thumbnail", processor, pool, config).await
}

async fn upload_circular_thumbnail(
    mut payload: Multipart, 
    image_type: &str, 
    processor: ImageProcessor,
    pool: web::Data<PgPool>,
    config: web::Data<Config>
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
    
    // 파일 크기 검증
    if processor.get_file_size_mb(&image_data) > config.max_file_size_mb {
        return Ok(HttpResponse::BadRequest().json(ImageResponse {
            success: false,
            message: "파일 크기는 30MB를 초과할 수 없습니다".to_string(),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    
    // 원형 썸네일 처리 (크롭 + 원형 마스킹 + WebP 변환)
    let processed_data = match processor.process_circular_thumbnail(&image_data) {
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
    
    // 업로드 디렉토리 생성 (./ 제거)
    let upload_dir = config.get_upload_path(image_type).trim_start_matches("./").to_string();
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
    
    // 파일 저장 (WebP)
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

    // 원본 파일 저장 (원본 확장자 유지)
    let original_ext = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let original_uuid = Uuid::new_v4().to_string()[..8].to_string();
    let original_filename = format!("{}_{}_{}.{}", image_type, original_uuid, timestamp, original_ext);
    let original_upload_dir = config.get_original_upload_path(image_type).trim_start_matches("./").to_string();
    if let Err(e) = fs::create_dir_all(&original_upload_dir) {
        return Ok(HttpResponse::InternalServerError().json(ImageResponse {
            success: false,
            message: format!("원본 디렉토리 생성 실패: {}", e),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    let original_filepath = format!("{}/{}", original_upload_dir, original_filename);
    if let Err(e) = fs::write(&original_filepath, &image_data) {
        return Ok(HttpResponse::InternalServerError().json(ImageResponse {
            success: false,
            message: format!("원본 파일 저장 실패: {}", e),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }

    // DB에 원본 이미지 정보 저장
    let db = database::Database { pool: pool.get_ref().clone() };
    let orig_size = processor.get_file_size_mb(&image_data);
    let (orig_width, orig_height, orig_format) = match processor.get_image_info(&image_data) {
        Ok(info) => info,
        Err(_) => (0, 0, original_ext.to_string()),
    };
    let original_id = db.save_original_image(
        &original_filename,
        &filename,
        &original_filepath.trim_start_matches("./"),
        orig_size,
        Some(orig_width),
        Some(orig_height),
        &orig_format,
    ).await.map_err(|e| actix_web::error::ErrorInternalServerError(format!("원본 DB 저장 실패: {}", e)))?;

    // DB에 WebP 이미지 정보 저장
    // WebP 이미지 정보 추출
    let (webp_width, webp_height, _) = match processor.get_image_info(&processed_data) {
        Ok(info) => info,
        Err(_) => (0, 0, "webp".to_string()),
    };
    let webp_size = processor.get_file_size_mb(&processed_data);
    db.save_webp_image(
        original_id,
        &webp_filename,
        &filepath.trim_start_matches("./"),
        webp_size,
        Some(webp_width),
        Some(webp_height),
        image_type,
    ).await.map_err(|e| actix_web::error::ErrorInternalServerError(format!("WebP DB 저장 실패: {}", e)))?;

    Ok(HttpResponse::Ok().json(ImageResponse {
        success: true,
        message: "원형 썸네일 생성 성공".to_string(),
        filename: Some(webp_filename.clone()),
        size_mb: Some(webp_size),
        width: Some(webp_width),
        height: Some(webp_height),
        format: Some("webp".to_string()),
        url: Some(config.get_file_url(&webp_filename)),
    }))
}

async fn upload_image(
    mut payload: Multipart, 
    image_type: &str, 
    processor: ImageProcessor,
    pool: web::Data<PgPool>,
    config: web::Data<Config>
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
    
    // 파일 크기 검증 (설정에서 가져온 제한)
    if processor.get_file_size_mb(&image_data) > config.max_file_size_mb {
        return Ok(HttpResponse::BadRequest().json(ImageResponse {
            success: false,
            message: "파일 크기는 30MB를 초과할 수 없습니다".to_string(),
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
    
    // 업로드 디렉토리 생성 (./ 제거)
    let upload_dir = config.get_upload_path(image_type).trim_start_matches("./").to_string();
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
    
    // 파일 저장 (WebP)
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

    // 원본 파일 저장 (원본 확장자 유지)
    let original_ext = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let original_uuid = Uuid::new_v4().to_string()[..8].to_string();
    let original_filename = format!("{}_{}_{}.{}", image_type, original_uuid, timestamp, original_ext);
    let original_upload_dir = config.get_original_upload_path(image_type).trim_start_matches("./").to_string();
    if let Err(e) = fs::create_dir_all(&original_upload_dir) {
        return Ok(HttpResponse::InternalServerError().json(ImageResponse {
            success: false,
            message: format!("원본 디렉토리 생성 실패: {}", e),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }
    let original_filepath = format!("{}/{}", original_upload_dir, original_filename);
    if let Err(e) = fs::write(&original_filepath, &image_data) {
        return Ok(HttpResponse::InternalServerError().json(ImageResponse {
            success: false,
            message: format!("원본 파일 저장 실패: {}", e),
            filename: None,
            size_mb: None,
            width: None,
            height: None,
            format: None,
            url: None,
        }));
    }

    // DB에 원본 이미지 정보 저장
    let db = database::Database { pool: pool.get_ref().clone() };
    let orig_size = processor.get_file_size_mb(&image_data);
    let (orig_width, orig_height, orig_format) = match processor.get_image_info(&image_data) {
        Ok(info) => info,
        Err(_) => (0, 0, original_ext.to_string()),
    };
    let original_id = db.save_original_image(
        &original_filename,
        &filename,
        &original_filepath.trim_start_matches("./"),
        orig_size,
        Some(orig_width),
        Some(orig_height),
        &orig_format,
    ).await.map_err(|e| actix_web::error::ErrorInternalServerError(format!("원본 DB 저장 실패: {}", e)))?;

    // DB에 WebP 이미지 정보 저장
    // WebP 이미지 정보 추출
    let (webp_width, webp_height, _) = match processor.get_image_info(&processed_data) {
        Ok(info) => info,
        Err(_) => (0, 0, "webp".to_string()),
    };
    let webp_size = processor.get_file_size_mb(&processed_data);
    db.save_webp_image(
        original_id,
        &webp_filename,
        &filepath.trim_start_matches("./"),
        webp_size,
        Some(webp_width),
        Some(webp_height),
        image_type,
    ).await.map_err(|e| actix_web::error::ErrorInternalServerError(format!("WebP DB 저장 실패: {}", e)))?;

    Ok(HttpResponse::Ok().json(ImageResponse {
        success: true,
        message: "이미지 업로드 성공".to_string(),
        filename: Some(webp_filename.clone()),
        size_mb: Some(webp_size),
        width: Some(webp_width),
        height: Some(webp_height),
        format: Some("webp".to_string()),
        url: Some(config.get_file_url(&webp_filename)),
    }))
}

async fn get_image_info(path: web::Path<String>, config: web::Data<Config>) -> Result<HttpResponse> {
    let filename = path.into_inner();
    
    // 파일 경로 찾기
    let filepath = find_image_file(&filename, &config);
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
        url: Some(config.get_file_url(&filename)),
    }))
}

async fn download_image(path: web::Path<String>, config: web::Data<Config>) -> Result<HttpResponse> {
    let filename = path.into_inner();
    
    // 파일 경로 찾기
    let filepath = find_image_file(&filename, &config);
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

async fn download_original_image(path: web::Path<String>, config: web::Data<Config>) -> Result<HttpResponse> {
    let filename = path.into_inner();
    
    // 원본 파일 경로 찾기
    let filepath = find_original_image_file(&filename, &config);
    if filepath.is_empty() {
        return Ok(HttpResponse::NotFound().json(ImageResponse {
            success: false,
            message: "원본 파일을 찾을 수 없습니다".to_string(),
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
                message: format!("원본 파일 읽기 실패: {}", e),
                filename: None,
                size_mb: None,
                width: None,
                height: None,
                format: None,
                url: None,
            }));
        }
    };
    
    // 파일 확장자에 따른 content-type 설정
    let content_type = match Path::new(&filename).extension().and_then(|e| e.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    
    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .body(file_data))
}

fn find_image_file(filename: &str, config: &Config) -> String {
    // 썸네일 디렉토리에서 검색
    let thumbnail_path = format!("{}/{}", config.get_upload_path("thumbnail"), filename);
    if Path::new(&thumbnail_path).exists() {
        return thumbnail_path;
    }
    
    // 지도 디렉토리에서 검색
    let map_path = format!("{}/{}", config.get_upload_path("map"), filename);
    if Path::new(&map_path).exists() {
        return map_path;
    }
    
    // 생성된 썸네일 디렉토리에서 검색
    let generated_thumbnail_path = format!("{}/{}", config.get_upload_path("generated_thumbnail"), filename);
    if Path::new(&generated_thumbnail_path).exists() {
        return generated_thumbnail_path;
    }
    
    String::new()
}

fn find_original_image_file(filename: &str, config: &Config) -> String {
    // 썸네일 원본 디렉토리에서 검색
    let thumbnail_original_path = format!("{}/{}", config.get_original_upload_path("thumbnail"), filename);
    if Path::new(&thumbnail_original_path).exists() {
        return thumbnail_original_path;
    }
    
    // 지도 원본 디렉토리에서 검색
    let map_original_path = format!("{}/{}", config.get_original_upload_path("map"), filename);
    if Path::new(&map_original_path).exists() {
        return map_original_path;
    }
    
    String::new()
}

async fn list_images(
    pool: web::Data<PgPool>,
    query: web::Query<std::collections::HashMap<String, String>>
) -> Result<HttpResponse> {
    let image_type = query.get("type");
    
    let rows = if let Some(img_type) = image_type {
        sqlx::query_as::<_, database::ImageInfo>(
            r#"
            SELECT id, filename, original_filename, file_path, file_size_mb, 
                   width, height, format, image_type, created_at, updated_at
            FROM bigpicture.images 
            WHERE image_type = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(img_type)
        .fetch_all(pool.get_ref())
        .await
    } else {
        sqlx::query_as::<_, database::ImageInfo>(
            r#"
            SELECT id, filename, original_filename, file_path, file_size_mb, 
                   width, height, format, image_type, created_at, updated_at
            FROM bigpicture.images 
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(pool.get_ref())
        .await
    };
    
    match rows {
        Ok(images) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "이미지 목록 조회 성공",
                "count": images.len(),
                "images": images
            })))
        }
        Err(e) => {
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("이미지 목록 조회 실패: {}", e)
            })))
        }
    }
}

async fn get_image_stats(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    // 전체 통계
    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bigpicture.images")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0);
    
    let total_size: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(file_size_mb), 0) FROM bigpicture.images")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0.0);
    
    // 타입별 통계
    let thumbnail_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bigpicture.images WHERE image_type = 'thumbnail'")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0);
    
    let thumbnail_size: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(file_size_mb), 0) FROM bigpicture.images WHERE image_type = 'thumbnail'")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0.0);
    
    let map_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bigpicture.images WHERE image_type = 'map'")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0);
    
    let map_size: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(file_size_mb), 0) FROM bigpicture.images WHERE image_type = 'map'")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0.0);
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "이미지 통계 조회 성공",
        "stats": {
            "total": {
                "count": total_count,
                "size_mb": total_size
            },
            "thumbnail": {
                "count": thumbnail_count,
                "size_mb": thumbnail_size
            },
            "map": {
                "count": map_count,
                "size_mb": map_size
            }
        }
    })))
} 