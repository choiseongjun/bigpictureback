use actix_web::{error::ResponseError, http::StatusCode, HttpResponse, Result};
use log::{error, warn, info};
use serde_json::json;

pub struct ErrorHandler;

impl ErrorHandler {
    pub fn log_and_respond(
        status: StatusCode,
        message: &str,
        error_details: Option<&str>,
        request_info: Option<&str>,
    ) -> HttpResponse {
        let status_code = status.as_u16();
        
        // 에러 로깅
        match status_code {
            400 => {
                error!("🚨 400 Bad Request - {}", message);
                if let Some(details) = error_details {
                    error!("   📋 상세 에러: {}", details);
                }
                if let Some(req_info) = request_info {
                    error!("   📡 요청 정보: {}", req_info);
                }
            }
            401 => {
                warn!("🔒 401 Unauthorized - {}", message);
                if let Some(details) = error_details {
                    warn!("   📋 상세 에러: {}", details);
                }
            }
            403 => {
                warn!("🚫 403 Forbidden - {}", message);
                if let Some(details) = error_details {
                    warn!("   📋 상세 에러: {}", details);
                }
            }
            404 => {
                info!("🔍 404 Not Found - {}", message);
            }
            422 => {
                error!("📝 422 Unprocessable Entity - {}", message);
                if let Some(details) = error_details {
                    error!("   📋 상세 에러: {}", details);
                }
            }
            500 => {
                error!("💥 500 Internal Server Error - {}", message);
                if let Some(details) = error_details {
                    error!("   📋 상세 에러: {}", details);
                }
            }
            _ => {
                error!("❓ {} {} - {}", status_code, status.canonical_reason().unwrap_or("Unknown"), message);
                if let Some(details) = error_details {
                    error!("   📋 상세 에러: {}", details);
                }
            }
        }

        // JSON 응답 생성
        let response_body = json!({
            "success": false,
            "error": {
                "code": status_code,
                "message": message,
                "status": status.canonical_reason().unwrap_or("Unknown")
            }
        });

        HttpResponse::build(status).json(response_body)
    }

    pub fn bad_request(message: &str, details: Option<&str>, request_info: Option<&str>) -> HttpResponse {
        Self::log_and_respond(StatusCode::BAD_REQUEST, message, details, request_info)
    }

    pub fn unauthorized(message: &str, details: Option<&str>) -> HttpResponse {
        Self::log_and_respond(StatusCode::UNAUTHORIZED, message, details, None)
    }

    pub fn forbidden(message: &str, details: Option<&str>) -> HttpResponse {
        Self::log_and_respond(StatusCode::FORBIDDEN, message, details, None)
    }

    pub fn not_found(message: &str) -> HttpResponse {
        Self::log_and_respond(StatusCode::NOT_FOUND, message, None, None)
    }

    pub fn unprocessable_entity(message: &str, details: Option<&str>) -> HttpResponse {
        Self::log_and_respond(StatusCode::UNPROCESSABLE_ENTITY, message, details, None)
    }

    pub fn internal_server_error(message: &str, details: Option<&str>) -> HttpResponse {
        Self::log_and_respond(StatusCode::INTERNAL_SERVER_ERROR, message, details, None)
    }
} 