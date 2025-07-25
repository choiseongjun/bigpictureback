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
use jsonwebtoken::{encode, EncodingKey, Header, decode, DecodingKey, Validation};
use base64::Engine;

use crate::image_processor::ImageProcessor;
use crate::database::{Database, Member, AuthProvider};
use crate::config::Config;
use crate::s3_service::S3Service;
use crate::s3_routes::{upload_image_s3, upload_circular_thumbnail_s3_internal};
use crate::error_handler::ErrorHandler;

// 구글 ID 토큰 페이로드 구조체
#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleIdTokenPayload {
    pub iss: String,           // issuer (Google)
    pub sub: String,           // subject (Google user ID)
    pub aud: String,           // audience (client ID)
    pub exp: i64,              // expiration time
    pub iat: i64,              // issued at
    pub email: String,         // user email
    pub email_verified: bool,  // email verification status
    pub name: Option<String>,  // user name
    pub picture: Option<String>, // profile picture URL
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub locale: Option<String>,
}

// 구글 공개키 구조체
#[derive(Debug, Serialize, Deserialize)]
pub struct GooglePublicKey {
    pub kid: String,
    pub e: String,
    pub n: String,
    pub alg: String,
    pub kty: String,
    #[serde(rename = "use")]
    pub use_field: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleKeysResponse {
    pub keys: Vec<GooglePublicKey>,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
    pub code: i32,
    pub message: String,
}

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

#[derive(Deserialize)]
pub struct RegisterMember {
    pub email: String,
    pub nickname: String,
    pub profile_image_url: Option<String>,
    pub region: Option<String>,
    pub gender: Option<String>,
    pub birth_year: Option<i32>,
    pub personality_type: Option<String>,
    pub interests: Option<Vec<String>>,
    pub hobbies: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct RegisterSocialMember {
    pub email: String,
    pub nickname: String,
    pub provider_type: String, // "google", "kakao", "email"
    pub provider_id: String,
    pub provider_email: Option<String>,
    pub password: Option<String>, // 이메일 로그인시에만 필요
    pub profile_image_url: Option<String>,
    pub region: Option<String>,
    pub gender: Option<String>,
    pub birth_year: Option<i32>,
    pub personality_type: Option<String>,
    pub interests: Option<Vec<String>>,
    pub hobbies: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SocialLoginRequest {
    pub provider_type: String,
    pub provider_id: String,
    pub provider_email: Option<String>,
    pub nickname: Option<String>,
    pub profile_image_url: Option<String>,
}

#[derive(Deserialize)]
pub struct GoogleIdTokenRequest {
    pub id_token: String,
    pub nickname: Option<String>,
    pub profile_image_url: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateMarkerRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub emotion_tag: String,
    pub description: String,
    pub thumbnail_img: Option<String>,
    pub images: Option<Vec<CreateMarkerImageRequest>>,
}

#[derive(Deserialize)]
pub struct CreateMarkerImageRequest {
    pub image_url: String,
    pub image_type: String, // thumbnail, detail, gallery
    pub image_order: Option<i32>,
    pub is_primary: Option<bool>,
}

#[derive(Deserialize)]
pub struct AddMarkerImageRequest {
    pub image_url: String,
    pub image_type: String, // thumbnail, detail, gallery
    pub image_order: Option<i32>,
    pub is_primary: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateMarkerImageOrderRequest {
    pub image_order: i32,
}

#[derive(Serialize)]
pub struct MarkerImageResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct MarkerResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct MarkerReactionResponse {
    pub success: bool,
    pub message: String,
    pub likes: i32,
    pub dislikes: i32,
    pub is_liked: Option<bool>,
    pub is_disliked: Option<bool>,
}

#[derive(Serialize)]
pub struct MarkerBookmarkResponse {
    pub success: bool,
    pub message: String,
    pub is_bookmarked: bool,
}

#[derive(Serialize)]
pub struct GoogleIdTokenResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
    #[serde(rename = "token")]
    pub token: Option<String>,
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "isNewUser")]
    pub is_new_user: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListMembersQuery {
    pub limit: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // subject (user id)
    pub email: String,
    pub exp: usize, // 만료시간 (timestamp)
}

fn create_jwt(user_id: i64, email: &str, config: &Config) -> Result<String, jsonwebtoken::errors::Error> {
    use chrono::Duration;
    let expiration = Utc::now() + Duration::hours(24);
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp: expiration.timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
}

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config
        .service(
            web::scope("/api")
                .route("/health", web::get().to(health_check))
                .route("/markers", web::get().to(get_markers))
                .route("/markers", web::post().to(
                    |db, payload, config, req| create_marker(db, payload, config, req)
                ))
                .route("/markers/feed", web::get().to(get_markers_feed))
                .route("/markers/cluster", web::get().to(get_markers_cluster))
                .route("/markers/rank", web::get().to(get_markers_rank))
                .route("/markers/{id}", web::get().to(get_marker_detail))
                .route("/markers/{id}/like", web::post().to(toggle_marker_like))
                .route("/markers/{id}/dislike", web::post().to(toggle_marker_dislike))
                .route("/markers/{id}/bookmark", web::post().to(toggle_marker_bookmark))
                .route("/markers/{id}/view", web::post().to(add_marker_view))
                .route("/markers/{id}/images", web::get().to(get_marker_images))
                .route("/markers/{id}/images", web::post().to(add_marker_image))
                .route("/markers/{id}/images/{image_id}", web::delete().to(delete_marker_image))
                .route("/markers/{id}/images/{image_id}/primary", web::put().to(set_marker_primary_image))
                .route("/markers/{id}/images/{image_id}/order", web::put().to(update_marker_image_order))
                .route("/members/{id}/markers/created", web::get().to(get_member_created_markers))
                .route("/members/{id}/markers/liked", web::get().to(get_member_liked_markers))
                .route("/members/{id}/markers/bookmarked", web::get().to(get_member_bookmarked_markers))
                .route("/members/{id}/markers/connect", web::post().to(connect_member_to_marker))
                .route("/members/{id}/markers/interactions", web::get().to(get_member_marker_interactions))
                .route("/members/{id}/markers/interactions/{interaction_type}", web::get().to(get_member_markers_by_interaction))
                .route("/members/{id}/markers/with-details", web::get().to(get_member_markers_with_details))
                .route("/members/{id}/markers/stats", web::get().to(get_member_marker_stats))
                .route("/members", web::post().to(register_member))
                .route("/members", web::get().to(list_members))
                .route("/members/me", web::get().to(
                    |db, config, req| get_me(db, config, req)
                ))
                .route("/members/{id}", web::get().to(get_member_by_id))
                .route("/members/{id}/with-markers", web::get().to(get_member_with_markers))
                .route("/members/{id}/with-marker-details", web::get().to(get_member_with_marker_details))
                .route("/members/{id}/with-stats", web::get().to(get_member_with_stats))
                .route("/auth/register", web::post().to(
                    |db, payload, config| register_social_member(db, payload, config)
                ))
                .route("/auth/login", web::post().to(
                    |db, payload, config| login_member(db, payload, config)
                ))
                .route("/auth/social-login", web::post().to(
                    |db, payload, config| social_login(db, payload, config)
                ))
                .route("/auth/google-id-token", web::post().to(
                    |db, payload, config| google_id_token_login(db, payload, config)
                ))
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
                        .route("/upload/normal", web::post().to(upload_thumbnail_s3))
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
    my: Option<bool>, // 추가: 내 마커만 표시 (기본 false)
}

#[derive(Deserialize)]
pub struct MarkersFeedQuery {
    page: Option<i32>,
    limit: Option<i32>,
    emotion_tags: Option<String>,
    min_likes: Option<i32>,
    min_views: Option<i32>,
    user_id: Option<i64>, // 특정 사용자의 마커만 조회
}

async fn get_markers(
    query: web::Query<MarkersQuery>,
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
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
    info!("   - my: {:?}", query.my);
    
    let db = Database { pool: pool.get_ref().clone() };
    
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

    // 내 마커만 조회 옵션 처리
    let mut user_id: Option<i64> = None;
    if query.my.unwrap_or(false) {
        // 토큰에서 user_id 추출
        if let Ok(uid) = extract_user_id_from_token(&req, &config) {
            user_id = Some(uid);
        } else {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "success": false,
                "message": "내 마커만 조회하려면 로그인(JWT)이 필요합니다."
            })));
        }
    }
    
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
        user_id, // 추가: user_id 전달
    ).await {
        Ok(markers) => {
            info!("✅ 마커 조회 성공: {}개 마커 반환", markers.len());
            
            // 각 마커에 이미지 정보 추가
            let mut formatted_markers = Vec::new();
            for marker in &markers {
                // 마커 이미지 조회
                let images = match db.get_marker_images(marker.id).await {
                    Ok(images) => images,
                    Err(e) => {
                        warn!("⚠️ 마커 {} 이미지 조회 실패: {}", marker.id, e);
                        vec![]
                    }
                };
                
                let formatted_images: Vec<serde_json::Value> = images.iter()
                    .map(|image| serde_json::json!({
                        "id": image.id,
                        "markerId": image.marker_id,
                        "imageType": image.image_type,
                        "imageUrl": image.image_url,
                        "imageOrder": image.image_order,
                        "isPrimary": image.is_primary,
                        "createdAt": image.created_at,
                        "updatedAt": image.updated_at
                    }))
                    .collect();
                
                let mut marker_data = marker_to_camelcase_json(marker);
                if let Some(marker_obj) = marker_data.as_object_mut() {
                    marker_obj.insert("images".to_string(), serde_json::Value::Array(formatted_images));
                }
                
                formatted_markers.push(marker_data);
            }
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": formatted_markers,
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
    let processor = ImageProcessor::new(150, 150, 85);
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
                        return Ok(ErrorHandler::bad_request(
                            "지원되지 않는 이미지 형식입니다. (jpg, jpeg, png, gif, bmp, webp)",
                            Some(&format!("파일명: {}", filename)),
                            Some("원형 썸네일 업로드 - 파일 형식 검증 실패")
                        ));
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
        return Ok(ErrorHandler::bad_request(
            "파일 크기는 30MB를 초과할 수 없습니다",
            Some(&format!("현재 크기: {:.2}MB", processor.get_file_size_mb(&image_data))),
            Some("원형 썸네일 업로드 - 파일 크기 초과")
        ));
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
    let db = Database { pool: pool.get_ref().clone() };
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
    let db = Database { pool: pool.get_ref().clone() };
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
        sqlx::query_as::<_, crate::database::ImageInfo>(
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
        sqlx::query_as::<_, crate::database::ImageInfo>(
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

async fn register_member(
    db: web::Data<Database>,
    payload: web::Json<RegisterMember>,
) -> Result<HttpResponse> {
    let input = payload.into_inner();
    let member_result = db.create_member(
        &input.email,
        &input.nickname,
        input.profile_image_url.as_deref(),
        input.region.as_deref(),
        input.gender.as_deref(),
        input.birth_year,
        input.personality_type.as_deref(),
    ).await;
    match member_result {
        Ok(member) => {
            // 관심사/취미 연결
            if let Some(interests) = &input.interests {
                let _ = db.add_member_interests(member.id, interests).await;
            }
            if let Some(hobbies) = &input.hobbies {
                let _ = db.add_member_hobbies(member.id, hobbies).await;
            }
            Ok(HttpResponse::Ok().json(ApiResponse {
                data: Some(member),
                code: 0,
                message: "회원 등록 성공".to_string(),
            }))
        },
        Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()> {
            data: None,
            code: 500,
            message: format!("회원 등록 실패: {}", e),
        })),
    }
}

async fn get_member_by_id(
    db: web::Data<Database>,
    path: web::Path<i32>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    match db.get_member_by_id(id.into()).await {
        Ok(Some(member)) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "data": member
        }))),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "success": false,
            "message": "회원이 존재하지 않습니다."
        }))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("회원 조회 실패: {}", e)
        }))),
    }
}

async fn list_members(
    db: web::Data<Database>,
    query: web::Query<ListMembersQuery>,
) -> Result<HttpResponse> {
    let limit = query.limit;
    match db.list_members(limit).await {
        Ok(members) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "data": members
        }))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("회원 목록 조회 실패: {}", e)
        }))),
    }
}

/// 소셜 로그인 회원가입 (구글, 카카오, 이메일)
async fn register_social_member(
    db: web::Data<Database>,
    payload: web::Json<RegisterSocialMember>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let input = payload.into_inner();
    
    info!("🔐 소셜 회원가입 요청:");
    info!("   - 이메일: {}", input.email);
    info!("   - 닉네임: {}", input.nickname);
    info!("   - 제공자: {}", input.provider_type);
    info!("   - 제공자 ID: {}", input.provider_id);
    
    // 1. 이미 존재하는 소셜 계정인지 확인
    if let Ok(Some((existing_member, existing_auth))) = db.find_member_by_social_provider(&input.provider_type, &input.provider_id).await {
        info!("✅ 기존 소셜 계정 발견, 로그인 처리");
        
        // 마지막 로그인 시간 업데이트
        if let Err(e) = db.update_last_login(existing_member.id).await {
            warn!("⚠️ 마지막 로그인 시간 업데이트 실패: {}", e);
        }
        
        // JWT 생성
        let token = create_jwt(existing_member.id, &existing_member.email, &config).unwrap_or_default();
        return Ok(HttpResponse::Ok().json(ApiResponse {
            data: Some(serde_json::json!({
                "member": member_to_camelcase_json(&existing_member),
                "authProvider": auth_provider_to_camelcase_json(&existing_auth),
                "isNewUser": false
            })),
            code: 0,
            message: "기존 계정으로 로그인 성공".to_string(),
        }));
    }
    
    // 2. 같은 이메일로 가입된 계정이 있는지 확인
    if let Ok(Some((existing_member, _existing_auth))) = db.find_member_by_email(&input.email).await {
        info!("📧 같은 이메일의 기존 계정 발견");
        
        // 기존 계정에 새로운 소셜 로그인 연결
        match db.link_social_provider(
            existing_member.id,
            &input.provider_type,
            &input.provider_id,
            input.provider_email.as_deref(),
        ).await {
            Ok(new_auth) => {
                info!("✅ 기존 계정에 소셜 로그인 연결 성공");
                // JWT 생성
                let token = create_jwt(existing_member.id, &existing_member.email, &config).unwrap_or_default();
                return Ok(HttpResponse::Ok().json(ApiResponse {
                    data: Some(serde_json::json!({
                        "member": member_to_camelcase_json(&existing_member),
                        "authProvider": auth_provider_to_camelcase_json(&new_auth),
                        "isNewUser": false
                    })),
                    code: 0,
                    message: "기존 계정에 소셜 로그인 연결 성공".to_string(),
                }));
            }
            Err(e) => {
                error!("❌ 소셜 로그인 연결 실패: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()> {
                    data: None,
                    code: 500,
                    message: format!("소셜 로그인 연결 실패: {}", e),
                }));
            }
        }
    }
    
    // 3. 새로운 회원 생성
    let result = match input.provider_type.as_str() {
        "email" => {
            // 이메일/비밀번호 회원가입
            let password_hash = input.password.ok_or_else(|| {
                actix_web::error::ErrorBadRequest("이메일 로그인시 비밀번호가 필요합니다")
            })?;
            
            // 실제로는 비밀번호 해싱이 필요하지만 여기서는 간단히 처리
            db.create_email_member(
                &input.email,
                &input.nickname,
                &password_hash, // 실제로는 해시된 비밀번호
                input.profile_image_url.as_deref(),
                input.region.as_deref(),
                input.gender.as_deref(),
                input.birth_year,
                input.personality_type.as_deref(),
            ).await
        }
        "google" | "kakao" | "naver" | "meta" => {
            // 소셜 로그인 회원가입
            db.create_social_member(
                &input.email,
                &input.nickname,
                &input.provider_type,
                &input.provider_id,
                input.provider_email.as_deref(),
                input.profile_image_url.as_deref(),
                input.region.as_deref(),
                input.gender.as_deref(),
                input.birth_year,
                input.personality_type.as_deref(),
            ).await
        }
        _ => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()> {
                data: None,
                code: 400,
                message: "지원하지 않는 로그인 제공자입니다. (email, google, kakao, naver, meta)".to_string(),
            }));
        }
    };
    
    match result {
        Ok((member, auth_provider)) => {
            // 관심사/취미 연결
            if let Some(interests) = &input.interests {
                let _ = db.add_member_interests(member.id, interests).await;
            }
            if let Some(hobbies) = &input.hobbies {
                let _ = db.add_member_hobbies(member.id, hobbies).await;
            }
            info!("✅ 새로운 회원 생성 성공: ID {}", member.id);
            // JWT 생성
            let token = create_jwt(member.id, &member.email, &config).unwrap_or_default();
            Ok(HttpResponse::Ok().json(ApiResponse {
                data: Some(serde_json::json!({
                    "member": member_to_camelcase_json(&member),
                    "authProvider": auth_provider_to_camelcase_json(&auth_provider),
                    "isNewUser": true
                })),
                code: 0,
                message: "회원가입 성공".to_string(),
            }))
        }
        Err(e) => {
            error!("❌ 회원가입 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()> {
                data: None,
                code: 500,
                message: format!("회원가입 실패: {}", e),
            }))
        }
    }
}

/// 이메일/비밀번호 로그인
async fn login_member(
    db: web::Data<Database>,
    payload: web::Json<LoginRequest>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let input = payload.into_inner();
    
    info!("🔐 이메일 로그인 요청: {}", input.email);
    
    // 이메일로 회원 찾기
    match db.find_member_by_email(&input.email).await {
        Ok(Some((member, auth_provider))) => {
            // 비밀번호 검증 (실제로는 해시 비교가 필요)
            if auth_provider.provider_type == "email" {
                // 실제로는 bcrypt나 argon2로 비밀번호 검증
                if let Some(stored_hash) = &auth_provider.password_hash {
                    if stored_hash == &input.password { // 실제로는 해시 비교
                        // 마지막 로그인 시간 업데이트
                        if let Err(e) = db.update_last_login(member.id).await {
                            warn!("⚠️ 마지막 로그인 시간 업데이트 실패: {}", e);
                        }
                        // JWT 생성
                        let token = create_jwt(member.id, &member.email, &config).unwrap_or_default();
                        let access_token = generate_access_token(member.id, &member.email, &config);
                        let refresh_token = generate_refresh_token(member.id, &member.email, &config);
                        info!("✅ 이메일 로그인 성공: {}", input.email);
                        return Ok(HttpResponse::Ok().json(serde_json::json!({
                            "success": true,
                            "message": "로그인 성공",
                            "token": token,
                            "accessToken": access_token,
                            "refreshToken": refresh_token,
                            "data": {
                                "member": member_to_camelcase_json(&member),
                                "authProvider": auth_provider_to_camelcase_json(&auth_provider)
                            }
                        })));
                    }
                }
            }
            
            Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "success": false,
                "message": "이메일 또는 비밀번호가 올바르지 않습니다"
            })))
        }
        Ok(None) => {
            info!("❌ 존재하지 않는 이메일: {}", input.email);
            Ok(ErrorHandler::unauthorized(
                "이메일 또는 비밀번호가 올바르지 않습니다",
                Some(&format!("이메일: {}", input.email))
            ))
        }
        Err(e) => {
            error!("❌ 로그인 처리 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("로그인 처리 실패: {}", e)
            })))
        }
    }
}

/// 소셜 로그인 (기존 계정 확인)
async fn social_login(
    db: web::Data<Database>,
    payload: web::Json<SocialLoginRequest>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let input = payload.into_inner();
    
    info!("🔐 소셜 로그인 요청:");
    info!("   - 제공자: {}", input.provider_type);
    info!("   - 제공자 ID: {}", input.provider_id);
    
    // 소셜 제공자로 기존 회원 찾기
    match db.find_member_by_social_provider(&input.provider_type, &input.provider_id).await {
        Ok(Some((member, auth_provider))) => {
            // 마지막 로그인 시간 업데이트
            if let Err(e) = db.update_last_login(member.id).await {
                warn!("⚠️ 마지막 로그인 시간 업데이트 실패: {}", e);
            }
            // JWT 생성
            let token = create_jwt(member.id, &member.email, &config).unwrap_or_default();
            let access_token = generate_access_token(member.id, &member.email, &config);
            let refresh_token = generate_refresh_token(member.id, &member.email, &config);
            info!("✅ 소셜 로그인 성공: {}", member.email);
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "소셜 로그인 성공",
                "token": token,
                "accessToken": access_token,
                "refreshToken": refresh_token,
                "data": {
                    "member": member_to_camelcase_json(&member),
                    "authProvider": auth_provider_to_camelcase_json(&auth_provider)
                }
            })))
        }
        Ok(None) => {
            info!("❌ 등록되지 않은 소셜 계정");
            Ok(HttpResponse::NotFound().json(serde_json::json!({
                "success": false,
                "message": "등록되지 않은 소셜 계정입니다. 회원가입을 먼저 진행해주세요.",
                "data": {
                    "provider_type": input.provider_type,
                    "provider_id": input.provider_id,
                    "provider_email": input.provider_email,
                    "nickname": input.nickname,
                    "profile_image_url": input.profile_image_url
                }
            })))
        }
        Err(e) => {
            error!("❌ 소셜 로그인 처리 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("소셜 로그인 처리 실패: {}", e)
            })))
        }
    }
} 

async fn get_me(
    db: web::Data<Database>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok());
    if auth_header.is_none() || !auth_header.unwrap().starts_with("Bearer ") {
        return Ok(ErrorHandler::unauthorized(
            "No Bearer token",
            Some("Authorization 헤더가 없거나 Bearer 형식이 아닙니다")
        ));
    }
    let token = &auth_header.unwrap()[7..];
    let validation = Validation::default();
    let claims = match decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &validation,
    ) {
        Ok(data) => data.claims,
        Err(e) => {
            return Ok(ErrorHandler::unauthorized(
                "Invalid token",
                Some(&format!("토큰 검증 실패: {}", e))
            ));
        }
    };
    let user_id: i64 = match claims.sub.parse() {
        Ok(id) => id,
        Err(_) => {
            return Ok(ErrorHandler::unauthorized(
                "Invalid user id in token",
                Some(&format!("토큰의 사용자 ID 파싱 실패: {}", claims.sub))
            ));
        }
    };
    match db.get_member_by_id(user_id).await {
        Ok(Some(member)) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "data": member_to_camelcase_json(&member)
        }))),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "success": false,
            "message": "회원이 존재하지 않습니다."
        }))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("회원 조회 실패: {}", e)
        }))),
    }
} 

/// 구글 ID 토큰 검증 (간소화된 버전)
async fn verify_google_id_token_simple(id_token: &str) -> Result<GoogleIdTokenPayload, Box<dyn std::error::Error>> {
    // 1. ID 토큰을 헤더, 페이로드, 서명으로 분리
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid ID token format".into());
    }
    
    // 2. 페이로드 디코딩 (서명 검증 없이)
    let payload_json = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1])?;
    let payload: GoogleIdTokenPayload = serde_json::from_slice(&payload_json)?;
    
    // 3. 기본 검증만 수행
    let now = chrono::Utc::now().timestamp();
    if payload.exp < now {
        return Err("Token expired".into());
    }
    
    if !payload.email_verified {
        return Err("Email not verified".into());
    }
    
    Ok(payload)
}

/// 액세스 토큰 생성
fn generate_access_token(user_id: i64, email: &str, config: &Config) -> String {
    use chrono::Duration;
    let expiration = Utc::now() + Duration::hours(24);
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp: expiration.timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    ).unwrap_or_default()
}

/// 리프레시 토큰 생성
fn generate_refresh_token(user_id: i64, email: &str, config: &Config) -> String {
    use chrono::Duration;
    let expiration = Utc::now() + Duration::days(30); // 30일 유효
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp: expiration.timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    ).unwrap_or_default()
}

/// 구글 ID 토큰으로 로그인/회원가입
async fn google_id_token_login(
    db: web::Data<Database>,
    payload: web::Json<GoogleIdTokenRequest>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let input = payload.into_inner();
    
    info!("🔐 구글 ID 토큰 로그인 요청");
    
    // ID 토큰 검증
    let google_payload = match verify_google_id_token_simple(&input.id_token).await {
        Ok(payload) => {
            info!("✅ 구글 ID 토큰 검증 성공: {}", payload.email);
            payload
        }
        Err(e) => {
            error!("❌ 구글 ID 토큰 검증 실패: {}", e);
            return Ok(ErrorHandler::unauthorized(
                "ID 토큰 검증 실패",
                Some(&format!("구글 토큰 검증 오류: {}", e))
            ));
        }
    };
    
    // 1. 이미 존재하는 구글 계정인지 확인
    if let Ok(Some((existing_member, existing_auth))) = db.find_member_by_social_provider("google", &google_payload.sub).await {
        info!("✅ 기존 구글 계정 발견, 로그인 처리");
        
        // 마지막 로그인 시간 업데이트
        if let Err(e) = db.update_last_login(existing_member.id).await {
            warn!("⚠️ 마지막 로그인 시간 업데이트 실패: {}", e);
        }
        
        // JWT 생성
        let token = create_jwt(existing_member.id, &existing_member.email, &config).unwrap_or_default();
        let access_token = generate_access_token(existing_member.id, &existing_member.email, &config);
        let refresh_token = generate_refresh_token(existing_member.id, &existing_member.email, &config);
        return Ok(HttpResponse::Ok().json(GoogleIdTokenResponse {
            success: true,
            message: "기존 계정으로 로그인 성공".to_string(),
            data: Some(serde_json::json!({
                "member": member_to_camelcase_json(&existing_member),
                "authProvider": auth_provider_to_camelcase_json(&existing_auth),
                "googlePayload": google_payload_to_camelcase_json(&google_payload)
            })),
            token: Some(token),
            access_token: Some(access_token),
            refresh_token: Some(refresh_token),
            is_new_user: Some(false),
        }));
    }
    
    // 2. 같은 이메일로 가입된 계정이 있는지 확인
    if let Ok(Some((existing_member, _existing_auth))) = db.find_member_by_email(&google_payload.email).await {
        info!("📧 같은 이메일의 기존 계정 발견");
        
        // 기존 계정에 구글 로그인 연결
        match db.link_social_provider(
            existing_member.id,
            "google",
            &google_payload.sub,
            Some(&google_payload.email),
        ).await {
            Ok(new_auth) => {
                info!("✅ 기존 계정에 구글 로그인 연결 성공");
                // JWT 생성
                let token = create_jwt(existing_member.id, &existing_member.email, &config).unwrap_or_default();
                let access_token = generate_access_token(existing_member.id, &existing_member.email, &config);
                let refresh_token = generate_refresh_token(existing_member.id, &existing_member.email, &config);
                return Ok(HttpResponse::Ok().json(GoogleIdTokenResponse {
                    success: true,
                    message: "기존 계정에 구글 로그인 연결 성공".to_string(),
                    data: Some(serde_json::json!({
                        "member": member_to_camelcase_json(&existing_member),
                        "authProvider": auth_provider_to_camelcase_json(&new_auth),
                        "googlePayload": google_payload_to_camelcase_json(&google_payload)
                    })),
                    token: Some(token),
                    access_token: Some(access_token),
                    refresh_token: Some(refresh_token),
                    is_new_user: Some(false),
                }));
            }
            Err(e) => {
                error!("❌ 구글 로그인 연결 실패: {}", e);
                return Ok(HttpResponse::InternalServerError().json(GoogleIdTokenResponse {
                    success: false,
                    message: format!("구글 로그인 연결 실패: {}", e),
                    data: None,
                    token: None,
                    access_token: None,
                    refresh_token: None,
                    is_new_user: None,
                }));
            }
        }
    }
    
    // 3. 새로운 회원 생성
    let nickname = input.nickname
        .or(google_payload.name.clone())
        .unwrap_or_else(|| {
            // 이름이 없으면 이메일에서 추출
            google_payload.email.split('@').next().unwrap_or("user").to_string()
        });
    
    let profile_image_url = input.profile_image_url
        .or(google_payload.picture.clone());
    
    let result = db.create_social_member(
        &google_payload.email,
        &nickname,
        "google",
        &google_payload.sub,
        Some(&google_payload.email),
        profile_image_url.as_deref(),
        None, // region
        None, // gender
        None, // birth_year
        None, // personality_type
    ).await;
    
    match result {
        Ok((member, auth_provider)) => {
            info!("✅ 새로운 구글 회원 생성 성공: ID {}", member.id);
            // JWT 생성
            let token = create_jwt(member.id, &member.email, &config).unwrap_or_default();
            let access_token = generate_access_token(member.id, &member.email, &config);
            let refresh_token = generate_refresh_token(member.id, &member.email, &config);
            Ok(HttpResponse::Ok().json(GoogleIdTokenResponse {
                success: true,
                message: "구글 회원가입 성공".to_string(),
                data: Some(serde_json::json!({
                    "member": member_to_camelcase_json(&member),
                    "authProvider": auth_provider_to_camelcase_json(&auth_provider),
                    "googlePayload": google_payload_to_camelcase_json(&google_payload)
                })),
                token: Some(token),
                access_token: Some(access_token),
                refresh_token: Some(refresh_token),
                is_new_user: Some(true),
            }))
        }
        Err(e) => {
            error!("❌ 구글 회원가입 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(GoogleIdTokenResponse {
                success: false,
                message: format!("구글 회원가입 실패: {}", e),
                data: None,
                token: None,
                access_token: None,
                refresh_token: None,
                is_new_user: None,
            }))
        }
        }
}

// 마커 이미지 관련 핸들러들
async fn get_marker_images(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner() as i32;
    
    info!("🖼️ 마커 이미지 조회 요청: 마커 ID {}", marker_id);
    
    match db.get_marker_images(marker_id).await {
        Ok(images) => {
            info!("✅ 마커 이미지 조회 성공: {}개 이미지", images.len());
            let formatted_images: Vec<serde_json::Value> = images.iter()
                .map(|image| serde_json::json!({
                    "id": image.id,
                    "markerId": image.marker_id,
                    "imageType": image.image_type,
                    "imageUrl": image.image_url,
                    "imageOrder": image.image_order,
                    "isPrimary": image.is_primary,
                    "createdAt": image.created_at,
                    "updatedAt": image.updated_at
                }))
                .collect();
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 이미지 조회 성공",
                "data": formatted_images,
                "count": images.len()
            })))
        }
        Err(e) => {
            error!("❌ 마커 이미지 조회 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 이미지 조회 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

async fn add_marker_image(
    db: web::Data<Database>,
    path: web::Path<i64>,
    payload: web::Json<AddMarkerImageRequest>,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner() as i32;
    let input = payload.into_inner();
    
    info!("🖼️ 마커 이미지 추가 요청: 마커 ID {}, 이미지 타입 {}", marker_id, input.image_type);
    
    let image_order = input.image_order.unwrap_or(0);
    let is_primary = input.is_primary.unwrap_or(false);
    
    match db.add_marker_image(marker_id, &input.image_type, &input.image_url, image_order, is_primary).await {
        Ok(image_id) => {
            info!("✅ 마커 이미지 추가 성공: 이미지 ID {}", image_id);
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 이미지 추가 성공",
                "data": {
                    "imageId": image_id,
                    "markerId": marker_id,
                    "imageType": input.image_type,
                    "imageUrl": input.image_url,
                    "imageOrder": image_order,
                    "isPrimary": is_primary
                }
            })))
        }
        Err(e) => {
            error!("❌ 마커 이미지 추가 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 이미지 추가 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

async fn delete_marker_image(
    db: web::Data<Database>,
    path: web::Path<(i64, i32)>,
) -> Result<HttpResponse> {
    let (marker_id, image_id) = path.into_inner();
    let marker_id = marker_id as i32;
    
    info!("🗑️ 마커 이미지 삭제 요청: 마커 ID {}, 이미지 ID {}", marker_id, image_id);
    
    match db.delete_marker_image(image_id).await {
        Ok(deleted) => {
            if deleted {
                info!("✅ 마커 이미지 삭제 성공: 이미지 ID {}", image_id);
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "message": "마커 이미지 삭제 성공",
                    "data": {
                        "imageId": image_id,
                        "deleted": true
                    }
                })))
            } else {
                info!("⚠️ 마커 이미지가 존재하지 않음: 이미지 ID {}", image_id);
                Ok(ErrorHandler::not_found("마커 이미지를 찾을 수 없습니다"))
            }
        }
        Err(e) => {
            error!("❌ 마커 이미지 삭제 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 이미지 삭제 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

async fn set_marker_primary_image(
    db: web::Data<Database>,
    path: web::Path<(i64, i32)>,
) -> Result<HttpResponse> {
    let (marker_id, image_id) = path.into_inner();
    let marker_id = marker_id as i32;
    
    info!("⭐ 마커 대표 이미지 설정 요청: 마커 ID {}, 이미지 ID {}", marker_id, image_id);
    
    match db.set_marker_primary_image(marker_id, image_id).await {
        Ok(_) => {
            info!("✅ 마커 대표 이미지 설정 성공: 이미지 ID {}", image_id);
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 대표 이미지 설정 성공",
                "data": {
                    "markerId": marker_id,
                    "primaryImageId": image_id
                }
            })))
        }
        Err(e) => {
            error!("❌ 마커 대표 이미지 설정 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 대표 이미지 설정 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

async fn update_marker_image_order(
    db: web::Data<Database>,
    path: web::Path<(i64, i32)>,
    payload: web::Json<UpdateMarkerImageOrderRequest>,
) -> Result<HttpResponse> {
    let (marker_id, image_id) = path.into_inner();
    let marker_id = marker_id as i32;
    let input = payload.into_inner();
    
    info!("📝 마커 이미지 순서 변경 요청: 마커 ID {}, 이미지 ID {}, 새 순서 {}", marker_id, image_id, input.image_order);
    
    match db.update_marker_image_order(image_id, input.image_order).await {
        Ok(_) => {
            info!("✅ 마커 이미지 순서 변경 성공: 이미지 ID {}", image_id);
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 이미지 순서 변경 성공",
                "data": {
                    "imageId": image_id,
                    "newOrder": input.image_order
                }
            })))
        }
        Err(e) => {
            error!("❌ 마커 이미지 순서 변경 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 이미지 순서 변경 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

/// Member를 카멜케이스 JSON으로 변환
fn member_to_camelcase_json(member: &Member) -> serde_json::Value {
    serde_json::json!({
        "id": member.id,
        "email": member.email,
        "nickname": member.nickname,
        "profileImageUrl": member.profile_image_url,
        "region": member.region,
        "gender": member.gender,
        "age": member.age,
        "personalityType": member.personality_type,
        "isActive": member.is_active,
        "emailVerified": member.email_verified,
        "createdAt": member.created_at,
        "updatedAt": member.updated_at,
        "lastLoginAt": member.last_login_at
    })
}

/// AuthProvider를 카멜케이스 JSON으로 변환
fn auth_provider_to_camelcase_json(auth_provider: &AuthProvider) -> serde_json::Value {
    serde_json::json!({
        "id": auth_provider.id,
        "memberId": auth_provider.member_id,
        "providerType": auth_provider.provider_type,
        "providerId": auth_provider.provider_id,
        "providerEmail": auth_provider.provider_email,
        "passwordHash": auth_provider.password_hash,
        "createdAt": auth_provider.created_at,
        "updatedAt": auth_provider.updated_at
    })
}

/// GooglePayload를 카멜케이스 JSON으로 변환
fn google_payload_to_camelcase_json(payload: &GoogleIdTokenPayload) -> serde_json::Value {
    serde_json::json!({
        "email": payload.email,
        "name": payload.name,
        "picture": payload.picture,
        "givenName": payload.given_name,
        "familyName": payload.family_name
    })
}

/// JWT 토큰에서 유저 ID 추출
fn extract_user_id_from_token(req: &actix_web::HttpRequest, config: &Config) -> Result<i64, actix_web::Error> {
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok());
    if auth_header.is_none() || !auth_header.unwrap().starts_with("Bearer ") {
        return Err(actix_web::error::ErrorUnauthorized("No Bearer token"));
    }
    let token = &auth_header.unwrap()[7..];
    let validation = Validation::default();
    let claims = match decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &validation,
    ) {
        Ok(data) => data.claims,
        Err(e) => {
            return Err(actix_web::error::ErrorUnauthorized(format!("Invalid token: {}", e)));
        }
    };
    let user_id: i64 = match claims.sub.parse() {
        Ok(id) => id,
        Err(_) => {
            return Err(actix_web::error::ErrorUnauthorized("Invalid user id in token"));
        }
    };
    Ok(user_id)
}

/// Marker를 카멜케이스 JSON으로 변환
fn marker_to_camelcase_json(marker: &crate::database::Marker) -> serde_json::Value {
    // PostGIS WKT 형식에서 좌표 추출 (POINT(lng lat))
    let (latitude, longitude) = if let Some(location) = &marker.location {
        if location.starts_with("POINT(") && location.ends_with(")") {
            let coords = &location[6..location.len()-1]; // "POINT(" 제거하고 ")" 제거
            let parts: Vec<&str> = coords.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Ok(lng), Ok(lat)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    (lat, lng) // WKT는 (longitude latitude) 순서이므로 바꿔줌
                } else {
                    (0.0, 0.0)
                }
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    serde_json::json!({
        "id": marker.id,
        "memberId": marker.member_id,
        "latitude": latitude,
        "longitude": longitude,
        "emotionTag": marker.emotion_tag,
        "description": marker.description,
        "likes": marker.likes,
        "dislikes": marker.dislikes,
        "views": marker.views,
        "author": marker.author,
        "thumbnailImg": marker.thumbnail_img,
        "createdAt": marker.created_at,
        "updatedAt": marker.updated_at
    })
}

/// 마커 생성
async fn create_marker(
    db: web::Data<Database>,
    payload: web::Json<CreateMarkerRequest>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let input = payload.into_inner();
    
    // JWT 토큰에서 사용자 ID 추출
    let user_id = match extract_user_id_from_token(&req, &config) {
        Ok(id) => id,
        Err(_) => {
            return Ok(ErrorHandler::unauthorized(
                "로그인이 필요합니다. JWT 토큰을 확인해주세요.",
                Some("마커 생성 - 토큰 추출 실패")
            ));
        }
    };
    
    // 사용자 정보 조회
    let user = match db.get_member_by_id(user_id).await {
        Ok(Some(member)) => member,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(MarkerResponse {
                success: false,
                message: "사용자를 찾을 수 없습니다.".to_string(),
                data: None,
            }));
        }
        Err(e) => {
            error!("❌ 사용자 조회 실패: {}", e);
            return Ok(HttpResponse::InternalServerError().json(MarkerResponse {
                success: false,
                message: format!("사용자 조회 실패: {}", e),
                data: None,
            }));
        }
    };
    
    info!("📍 마커 생성 요청: 사용자 {} ({}), 위치 ({}, {})", user.nickname, user_id, input.latitude, input.longitude);
    
    // 이미지 정보 로깅
    if let Some(ref images) = input.images {
        info!("   - 이미지 {}개 포함", images.len());
        for (i, img) in images.iter().enumerate() {
            info!("     {}. {} (타입: {}, 순서: {}, 대표: {})", 
                i + 1, img.image_url, img.image_type, 
                img.image_order.unwrap_or(0), 
                img.is_primary.unwrap_or(false));
        }
    }
    
    match db.create_marker(
        user_id,
        input.latitude,
        input.longitude,
        &input.emotion_tag,
        &input.description,
        &user.nickname, // 실제 사용자 닉네임 사용
        input.thumbnail_img.as_deref(),
    ).await {
        Ok(marker) => {
            info!("✅ 마커 생성 성공: ID {}, 작성자 {}", marker.id, user.nickname);
            
            // 이미지들 추가
            let mut added_images = Vec::new();
            if let Some(images) = input.images {
                for (index, image_req) in images.into_iter().enumerate() {
                    let image_order = image_req.image_order.unwrap_or(index as i32);
                    let is_primary = image_req.is_primary.unwrap_or(index == 0); // 첫 번째 이미지를 기본 대표로 설정
                    
                    match db.add_marker_image(
                        marker.id,
                        &image_req.image_type,
                        &image_req.image_url,
                        image_order,
                        is_primary,
                    ).await {
                        Ok(image_id) => {
                            info!("✅ 이미지 추가 성공: ID {}, 타입 {}", image_id, image_req.image_type);
                            added_images.push(serde_json::json!({
                                "id": image_id,
                                "markerId": marker.id,
                                "imageType": image_req.image_type,
                                "imageUrl": image_req.image_url,
                                "imageOrder": image_order,
                                "isPrimary": is_primary
                            }));
                        }
                        Err(e) => {
                            error!("❌ 이미지 추가 실패: {}", e);
                            // 이미지 추가 실패해도 마커는 생성되었으므로 경고만 남김
                        }
                    }
                }
            }
            
            // 응답 데이터 구성
            let mut marker_data = marker_to_camelcase_json(&marker);
            if let Some(marker_obj) = marker_data.as_object_mut() {
                marker_obj.insert("images".to_string(), serde_json::Value::Array(added_images));
            }
            
            Ok(HttpResponse::Ok().json(MarkerResponse {
                success: true,
                message: "마커 생성 성공".to_string(),
                data: Some(marker_data),
            }))
        }
        Err(e) => {
            error!("❌ 마커 생성 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 생성 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

/// 마커 상세 정보 조회
async fn get_marker_detail(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner();
    
    info!("🔍 마커 상세 조회: 마커 {}", marker_id);
    
    match db.get_marker_detail(marker_id).await {
        Ok(Some(marker)) => {
            // 마커 이미지 정보도 함께 조회
            let images = match db.get_marker_images(marker_id as i32).await {
                Ok(images) => images,
                Err(e) => {
                    warn!("⚠️ 마커 이미지 조회 실패: {}", e);
                    vec![]
                }
            };
            
            let formatted_images: Vec<serde_json::Value> = images.iter()
                .map(|image| serde_json::json!({
                    "id": image.id,
                    "markerId": image.marker_id,
                    "imageType": image.image_type,
                    "imageUrl": image.image_url,
                    "imageOrder": image.image_order,
                    "isPrimary": image.is_primary,
                    "createdAt": image.created_at,
                    "updatedAt": image.updated_at
                }))
                .collect();
            
            let marker_data = serde_json::json!({
                "marker": marker_to_camelcase_json(&marker),
                "images": formatted_images
            });
            
            Ok(HttpResponse::Ok().json(MarkerResponse {
                success: true,
                message: "마커 상세 조회 성공".to_string(),
                data: Some(marker_data),
            }))
        }
        Ok(None) => {
            Ok(ErrorHandler::not_found("마커를 찾을 수 없습니다"))
        }
        Err(e) => {
            error!("❌ 마커 상세 조회 실패: {}", e);
            Ok(ErrorHandler::internal_server_error(
                "마커 상세 조회 실패",
                Some(&format!("데이터베이스 오류: {}", e))
            ))
        }
    }
}

/// 마커 좋아요 토글
async fn toggle_marker_like(
    db: web::Data<Database>,
    path: web::Path<i64>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner();
    let user_id = extract_user_id_from_token(&req, &config)?;
    
    info!("👍 마커 좋아요 토글: 마커 {}, 유저 {}", marker_id, user_id);
    
    match db.toggle_marker_reaction(user_id, marker_id, "liked").await {
        Ok((likes, dislikes)) => {
            Ok(HttpResponse::Ok().json(MarkerReactionResponse {
                success: true,
                message: "좋아요 처리 완료".to_string(),
                likes,
                dislikes,
                is_liked: Some(likes > 0),
                is_disliked: Some(dislikes > 0),
            }))
        }
        Err(e) => {
            error!("❌ 마커 좋아요 처리 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(MarkerReactionResponse {
                success: false,
                message: format!("좋아요 처리 실패: {}", e),
                likes: 0,
                dislikes: 0,
                is_liked: None,
                is_disliked: None,
            }))
        }
    }
}

/// 마커 싫어요 토글
async fn toggle_marker_dislike(
    db: web::Data<Database>,
    path: web::Path<i64>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner();
    let user_id = extract_user_id_from_token(&req, &config)?;
    
    info!("👎 마커 싫어요 토글: 마커 {}, 유저 {}", marker_id, user_id);
    
    match db.toggle_marker_reaction(user_id, marker_id, "disliked").await {
        Ok((likes, dislikes)) => {
            Ok(HttpResponse::Ok().json(MarkerReactionResponse {
                success: true,
                message: "싫어요 처리 완료".to_string(),
                likes,
                dislikes,
                is_liked: Some(likes > 0),
                is_disliked: Some(dislikes > 0),
            }))
        }
        Err(e) => {
            error!("❌ 마커 싫어요 처리 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(MarkerReactionResponse {
                success: false,
                message: format!("싫어요 처리 실패: {}", e),
                likes: 0,
                dislikes: 0,
                is_liked: None,
                is_disliked: None,
            }))
        }
    }
}

/// 마커 북마크 토글
async fn toggle_marker_bookmark(
    db: web::Data<Database>,
    path: web::Path<i64>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner();
    let user_id = extract_user_id_from_token(&req, &config)?;
    
    info!("🔖 마커 북마크 토글: 마커 {}, 유저 {}", marker_id, user_id);
    
    match db.toggle_marker_bookmark(user_id, marker_id).await {
        Ok(is_bookmarked) => {
            Ok(HttpResponse::Ok().json(MarkerBookmarkResponse {
                success: true,
                message: if is_bookmarked { "북마크 추가 완료".to_string() } else { "북마크 제거 완료".to_string() },
                is_bookmarked,
            }))
        }
        Err(e) => {
            error!("❌ 마커 북마크 처리 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(MarkerBookmarkResponse {
                success: false,
                message: format!("북마크 처리 실패: {}", e),
                is_bookmarked: false,
            }))
        }
    }
}

/// 마커 조회 기록 추가
async fn add_marker_view(
    db: web::Data<Database>,
    path: web::Path<i64>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let marker_id = path.into_inner();
    let user_id = extract_user_id_from_token(&req, &config)?;
    
    info!("👁️ 마커 조회 기록: 마커 {}, 유저 {}", marker_id, user_id);
    
    match db.add_marker_view(user_id, marker_id).await {
        Ok(_) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "조회 기록 추가 완료"
            })))
        }
        Err(e) => {
            error!("❌ 마커 조회 기록 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("조회 기록 실패: {}", e)
            })))
        }
    }
}

/// 유저가 생성한 마커 목록 조회
async fn get_member_created_markers(
    db: web::Data<Database>,
    path: web::Path<i64>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    let limit = query.get("limit").and_then(|l| l.parse::<i32>().ok());
    
    info!("📝 유저 생성 마커 조회: 유저 {}, 제한 {:?}", member_id, limit);
    
    match db.get_member_created_markers(member_id, limit).await {
        Ok(markers) => {
            let markers_json: Vec<serde_json::Value> = markers.iter()
                .map(|marker| marker_to_camelcase_json(marker))
                .collect();
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "생성한 마커 목록 조회 성공",
                "data": markers_json,
                "count": markers.len()
            })))
        }
        Err(e) => {
            error!("❌ 유저 생성 마커 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("생성한 마커 조회 실패: {}", e)
            })))
        }
    }
}

/// 유저가 좋아요한 마커 목록 조회
async fn get_member_liked_markers(
    db: web::Data<Database>,
    path: web::Path<i64>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    let limit = query.get("limit").and_then(|l| l.parse::<i32>().ok());
    
    info!("👍 유저 좋아요 마커 조회: 유저 {}, 제한 {:?}", member_id, limit);
    
    match db.get_member_liked_markers(member_id, limit).await {
        Ok(markers) => {
            let markers_json: Vec<serde_json::Value> = markers.iter()
                .map(|marker| marker_to_camelcase_json(marker))
                .collect();
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "좋아요한 마커 목록 조회 성공",
                "data": markers_json,
                "count": markers.len()
            })))
        }
        Err(e) => {
            error!("❌ 유저 좋아요 마커 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("좋아요한 마커 조회 실패: {}", e)
            })))
        }
    }
}

/// 유저가 북마크한 마커 목록 조회
async fn get_member_bookmarked_markers(
    db: web::Data<Database>,
    path: web::Path<i64>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    let limit = query.get("limit").and_then(|l| l.parse::<i32>().ok());
    
    info!("🔖 유저 북마크 마커 조회: 유저 {}, 제한 {:?}", member_id, limit);
    
    match db.get_member_bookmarked_markers(member_id, limit).await {
        Ok(markers) => {
            let markers_json: Vec<serde_json::Value> = markers.iter()
                .map(|marker| marker_to_camelcase_json(marker))
                .collect();
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "북마크한 마커 목록 조회 성공",
                "data": markers_json,
                "count": markers.len()
            })))
        }
        Err(e) => {
            error!("❌ 유저 북마크 마커 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("북마크한 마커 조회 실패: {}", e)
            })))
        }
    }
} 

/// 3번 사용자와 마커 연결
async fn connect_member_to_marker(
    db: web::Data<Database>,
    path: web::Path<i64>,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    let input = payload.into_inner();
    
    let marker_id = input.get("marker_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| actix_web::error::ErrorBadRequest("marker_id is required"))?;
    
    let interaction_type = input.get("interaction_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| actix_web::error::ErrorBadRequest("interaction_type is required"))?;
    
    info!("🔗 사용자 {}와 마커 {} 연결: {}", member_id, marker_id, interaction_type);
    
    match db.connect_member_to_marker(member_id, marker_id, interaction_type).await {
        Ok(_) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 연결 성공",
                "data": {
                    "member_id": member_id,
                    "marker_id": marker_id,
                    "interaction_type": interaction_type
                }
            })))
        }
        Err(e) => {
            error!("❌ 마커 연결 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("마커 연결 실패: {}", e)
            })))
        }
    }
}

/// 3번 사용자의 모든 마커 상호작용 조회
async fn get_member_marker_interactions(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    
    info!("🔍 사용자 {}의 모든 마커 상호작용 조회", member_id);
    
    match db.get_member_marker_interactions(member_id).await {
        Ok(interactions) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 상호작용 조회 성공",
                "data": interactions,
                "count": interactions.len()
            })))
        }
        Err(e) => {
            error!("❌ 마커 상호작용 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("마커 상호작용 조회 실패: {}", e)
            })))
        }
    }
}

/// 3번 사용자의 특정 상호작용 타입 마커 조회
async fn get_member_markers_by_interaction(
    db: web::Data<Database>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (member_id, interaction_type) = path.into_inner();
    
    info!("🔍 사용자 {}의 {} 상호작용 마커 조회", member_id, interaction_type);
    
    match db.get_member_markers_by_interaction(member_id, &interaction_type).await {
        Ok(interactions) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": format!("{} 상호작용 마커 조회 성공", interaction_type),
                "data": interactions,
                "count": interactions.len()
            })))
        }
        Err(e) => {
            error!("❌ {} 상호작용 마커 조회 실패: {}", interaction_type, e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("{} 상호작용 마커 조회 실패: {}", interaction_type, e)
            })))
        }
    }
}

/// 3번 사용자와 마커 상세 정보 함께 조회
async fn get_member_markers_with_details(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    
    info!("🔍 사용자 {}의 마커 상세 정보 조회", member_id);
    
    match db.get_member_markers_with_details(member_id).await {
        Ok(details) => {
            let formatted_details: Vec<serde_json::Value> = details.iter().map(|(member_marker, marker)| {
                serde_json::json!({
                    "interaction": {
                        "id": member_marker.id,
                        "member_id": member_marker.member_id,
                        "marker_id": member_marker.marker_id,
                        "interaction_type": member_marker.interaction_type,
                        "created_at": member_marker.created_at,
                        "updated_at": member_marker.updated_at
                    },
                    "marker": {
                        "id": marker.id,
                        "location": marker.location,
                        "emotion_tag": marker.emotion_tag,
                        "description": marker.description,
                        "likes": marker.likes,
                        "dislikes": marker.dislikes,
                        "views": marker.views,
                        "author": marker.author,
                        "thumbnail_img": marker.thumbnail_img
                    }
                })
            }).collect();
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 상세 정보 조회 성공",
                "data": formatted_details,
                "count": details.len()
            })))
        }
        Err(e) => {
            error!("❌ 마커 상세 정보 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("마커 상세 정보 조회 실패: {}", e)
            })))
        }
    }
}

/// 3번 사용자의 마커 상호작용 통계 조회
async fn get_member_marker_stats(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    
    info!("📊 사용자 {}의 마커 상호작용 통계 조회", member_id);
    
    match db.get_member_marker_stats(member_id).await {
        Ok(stats) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "마커 상호작용 통계 조회 성공",
                "data": stats
            })))
        }
        Err(e) => {
            error!("❌ 마커 상호작용 통계 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("마커 상호작용 통계 조회 실패: {}", e)
            })))
        }
    }
}

/// 유저 조회 (마커 정보 포함)
async fn get_member_with_markers(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    
    info!("👤 유저 {} 조회 (마커 정보 포함)", member_id);
    
    match db.get_member_with_markers(member_id).await {
        Ok(Some((member, markers))) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "유저 조회 성공 (마커 정보 포함)",
                "data": {
                    "member": member_to_camelcase_json(&member),
                    "markers": markers,
                    "marker_count": markers.len()
                }
            })))
        }
        Ok(None) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({
                "success": false,
                "message": "유저를 찾을 수 없습니다."
            })))
        }
        Err(e) => {
            error!("❌ 유저 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("유저 조회 실패: {}", e)
            })))
        }
    }
}

/// 유저 조회 (마커 상세 정보 포함)
async fn get_member_with_marker_details(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    
    info!("👤 유저 {} 조회 (마커 상세 정보 포함)", member_id);
    
    match db.get_member_with_marker_details(member_id).await {
        Ok(Some((member, marker_details))) => {
            let formatted_details: Vec<serde_json::Value> = marker_details.iter().map(|(member_marker, marker)| {
                serde_json::json!({
                    "interaction": {
                        "id": member_marker.id,
                        "member_id": member_marker.member_id,
                        "marker_id": member_marker.marker_id,
                        "interaction_type": member_marker.interaction_type,
                        "created_at": member_marker.created_at,
                        "updated_at": member_marker.updated_at
                    },
                    "marker": marker_to_camelcase_json(marker)
                })
            }).collect();
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "유저 조회 성공 (마커 상세 정보 포함)",
                "data": {
                    "member": member_to_camelcase_json(&member),
                    "marker_details": formatted_details,
                    "marker_count": marker_details.len()
                }
            })))
        }
        Ok(None) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({
                "success": false,
                "message": "유저를 찾을 수 없습니다."
            })))
        }
        Err(e) => {
            error!("❌ 유저 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("유저 조회 실패: {}", e)
            })))
        }
    }
}

/// 유저 조회 (마커 통계 포함)
async fn get_member_with_stats(
    db: web::Data<Database>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let member_id = path.into_inner();
    
    info!("👤 유저 {} 조회 (마커 통계 포함)", member_id);
    
    match db.get_member_with_stats(member_id).await {
        Ok(Some((member, stats))) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "유저 조회 성공 (마커 통계 포함)",
                "data": {
                    "member": member_to_camelcase_json(&member),
                    "marker_stats": stats
                }
            })))
        }
        Ok(None) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({
                "success": false,
                "message": "유저를 찾을 수 없습니다."
            })))
        }
        Err(e) => {
            error!("❌ 유저 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("유저 조회 실패: {}", e)
            })))
        }
    }
}

/// 피드용 마커 조회 (시간순 내림차순)
async fn get_markers_feed(
    query: web::Query<MarkersFeedQuery>,
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(20);
    
    info!("📱 피드 마커 조회 요청:");
    info!("   - 페이지: {}", page);
    info!("   - 제한: {}", limit);
    info!("   - 감성 태그: {:?}", query.emotion_tags);
    info!("   - 최소 좋아요: {:?}", query.min_likes);
    info!("   - 최소 조회수: {:?}", query.min_views);
    info!("   - 사용자 ID: {:?}", query.user_id);
    
    let db = Database { pool: pool.get_ref().clone() };
    
    // 감성 태그 파싱
    let emotion_tags = query.emotion_tags.as_ref().map(|tags| {
        let parsed_tags: Vec<String> = tags.split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        parsed_tags
    });
    
    match db.get_markers_feed(
        page,
        limit,
        emotion_tags,
        query.min_likes,
        query.min_views,
        query.user_id,
    ).await {
        Ok((markers, total_count)) => {
            info!("✅ 피드 마커 조회 성공: {}개 마커 반환 (전체: {}개)", markers.len(), total_count);
            
            // 각 마커에 이미지 정보 추가
            let mut formatted_markers = Vec::new();
            for marker in &markers {
                // 마커 이미지 조회
                let images = match db.get_marker_images(marker.id).await {
                    Ok(images) => images,
                    Err(e) => {
                        warn!("⚠️ 마커 {} 이미지 조회 실패: {}", marker.id, e);
                        vec![]
                    }
                };
                
                let formatted_images: Vec<serde_json::Value> = images.iter()
                    .map(|image| serde_json::json!({
                        "id": image.id,
                        "markerId": image.marker_id,
                        "imageType": image.image_type,
                        "imageUrl": image.image_url,
                        "imageOrder": image.image_order,
                        "isPrimary": image.is_primary,
                        "createdAt": image.created_at,
                        "updatedAt": image.updated_at
                    }))
                    .collect();
                
                let mut marker_data = marker_to_camelcase_json(marker);
                if let Some(marker_obj) = marker_data.as_object_mut() {
                    marker_obj.insert("images".to_string(), serde_json::Value::Array(formatted_images));
                }
                
                formatted_markers.push(marker_data);
            }
            
            // 페이지네이션 정보 계산
            let total_pages = (total_count as f64 / limit as f64).ceil() as i32;
            let has_next = page < total_pages;
            let has_prev = page > 1;
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": formatted_markers,
                "pagination": {
                    "currentPage": page,
                    "totalPages": total_pages,
                    "totalCount": total_count,
                    "limit": limit,
                    "hasNext": has_next,
                    "hasPrev": has_prev
                },
                "count": markers.len()
            })))
        }
        Err(e) => {
            error!("❌ 피드 마커 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("피드 마커 조회 실패: {}", e)
            })))
        }
    }
}

/// 마커 클러스터 조회
async fn get_markers_cluster(
    query: web::Query<MarkersQuery>,
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let db = Database { pool: pool.get_ref().clone() };
    // 파라미터 파싱
    let emotion_tags = query.emotion_tags.as_ref().map(|tags| {
        tags.split(',').map(|tag| tag.trim().to_string()).filter(|tag| !tag.is_empty()).collect::<Vec<_>>()
    });
    let sort_by = query.sort_by.as_deref();
    let sort_order = query.sort_order.as_deref();
    let mut user_id = None;
    if query.my.unwrap_or(false) {
        if let Ok(uid) = extract_user_id_from_token(&req, &config) {
            user_id = Some(uid);
        } else {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "success": false,
                "message": "내 마커만 표시하려면 로그인(JWT)이 필요합니다."
            })));
        }
    }
    match db.get_markers_cluster(
        query.lat, query.lng, query.lat_delta, query.lng_delta,
        emotion_tags, query.min_likes, query.min_views,
        sort_by, sort_order, query.limit, user_id
    ).await {
        Ok(mut clusters) => {
            // user_id가 있으면 각 마커에 isMine 추가
            if let Some(uid) = user_id {
                for cluster in clusters.iter_mut() {
                    if let Some(markers) = cluster.get_mut("markers") {
                        if let Some(arr) = markers.as_array_mut() {
                            for marker in arr.iter_mut() {
                                if let Some(obj) = marker.as_object_mut() {
                                    let is_mine = obj.get("memberId").and_then(|v| v.as_i64()).map(|mid| mid == uid).unwrap_or(false);
                                    obj.insert("isMine".to_string(), serde_json::json!(is_mine));
                                }
                            }
                        }
                    }
                }
            } else {
                // user_id 없으면 모두 false
                for cluster in clusters.iter_mut() {
                    if let Some(markers) = cluster.get_mut("markers") {
                        if let Some(arr) = markers.as_array_mut() {
                            for marker in arr.iter_mut() {
                                if let Some(obj) = marker.as_object_mut() {
                                    obj.insert("isMine".to_string(), serde_json::json!(false));
                                }
                            }
                        }
                    }
                }
            }
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": clusters,
                "count": clusters.len()
            })))
        },
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("마커 클러스터 조회 실패: {}", e)
        }))),
    }
}

#[derive(Deserialize)]
pub struct RankMarkersQuery {
    pub limit: Option<i32>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub emotion_tags: Option<String>,
    pub min_likes: Option<i32>,
    pub min_views: Option<i32>,
    pub my: Option<bool>,
}

async fn get_markers_rank(
    query: web::Query<RankMarkersQuery>,
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    info!("🏆 마커 순위 조회 요청:");
    info!("   - 제한: {:?}", query.limit);
    info!("   - 정렬 기준: {:?}", query.sort_by);
    info!("   - 정렬 순서: {:?}", query.sort_order);
    info!("   - 감성 태그: {:?}", query.emotion_tags);
    info!("   - 최소 좋아요: {:?}", query.min_likes);
    info!("   - 최소 조회수: {:?}", query.min_views);
    info!("   - 내 마커 포함: {:?}", query.my);
    let db = Database { pool: pool.get_ref().clone() };
    let emotion_tags = query.emotion_tags.as_ref().map(|tags| {
        tags.split(',').map(|tag| tag.trim().to_string()).filter(|tag| !tag.is_empty()).collect::<Vec<_>>()
    });
    let sort_by = query.sort_by.as_deref();
    let sort_order = query.sort_order.as_deref();
    let mut user_id: Option<i64> = None;
    if query.my.unwrap_or(false) {
        if let Ok(uid) = extract_user_id_from_token(&req, &config) {
            user_id = Some(uid);
        } else {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "success": false,
                "message": "내 마커만 조회하려면 로그인(JWT)이 필요합니다."
            })));
        }
    }
    match db.get_markers_rank(
        0.0, 0.0, 0.0, 0.0, // 좌표는 랭킹에 필요없으므로 더미값
        emotion_tags,
        query.min_likes,
        query.min_views,
        sort_by,
        sort_order,
        query.limit,
        user_id,
    ).await {
        Ok(markers) => {
            info!("✅ 마커 순위 조회 성공: {}개 마커 반환", markers.len());
            let mut formatted_markers = Vec::new();
            for marker in &markers {
                let images = match db.get_marker_images(marker.id).await {
                    Ok(images) => images,
                    Err(e) => {
                        warn!("⚠️ 마커 {} 이미지 조회 실패: {}", marker.id, e);
                        vec![]
                    }
                };
                let formatted_images: Vec<serde_json::Value> = images.iter()
                    .map(|image| serde_json::json!({
                        "id": image.id,
                        "markerId": image.marker_id,
                        "imageType": image.image_type,
                        "imageUrl": image.image_url,
                        "imageOrder": image.image_order,
                        "isPrimary": image.is_primary,
                        "createdAt": image.created_at,
                        "updatedAt": image.updated_at
                    }))
                    .collect();
                let mut marker_data = marker_to_camelcase_json(marker);
                if let Some(marker_obj) = marker_data.as_object_mut() {
                    marker_obj.insert("images".to_string(), serde_json::Value::Array(formatted_images));
                }
                formatted_markers.push(marker_data);
            }
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": formatted_markers,
                "count": markers.len()
            })))
        }
        Err(e) => {
            error!("❌ 마커 순위 조회 실패: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("마커 순위 조회 실패: {}", e)
            })))
        }
    }
}