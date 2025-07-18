use std::env;
use dotenv::dotenv;

#[derive(Debug, Clone)]
pub struct Config {
    // Database
    pub database_url: String,
    pub db_host: String,
    pub db_port: u16,
    pub db_user: String,
    pub db_password: String,
    pub db_name: String,
    
    // Server
    pub server_host: String,
    pub server_port: u16,
    
    // Image Processing
    pub thumbnail_max_width: u32,
    pub thumbnail_max_height: u32,
    pub thumbnail_quality: u8,
    pub map_max_width: u32,
    pub map_max_height: u32,
    pub map_quality: u8,
    
    // File Upload
    pub max_file_size_mb: f64,
    pub upload_dir: String,
    pub file_server_url: String,
    
    // S3
    pub s3_bucket_name: String,
    pub s3_region: String,
    pub s3_access_key_id: String,
    pub s3_secret_access_key: String,
    // JWT
    pub jwt_secret: String,
    
    // OAuth
    pub google_client_id: String,
    pub google_client_ids: Vec<String>,
}

impl Config {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // env.local을 먼저 로드하고, .env는 나중에 로드
        dotenv::from_filename("env.local").ok();
        dotenv().ok();
        
        Ok(Self {
            // Database
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://postgres:123@localhost:5432/bigpicture".to_string()),
            db_host: env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string()),
            db_port: env::var("DB_PORT")
                .unwrap_or_else(|_| "5432".to_string())
                .parse()
                .unwrap_or(5432),
            db_user: env::var("DB_USER").unwrap_or_else(|_| "postgres".to_string()),
            db_password: env::var("DB_PASSWORD").unwrap_or_else(|_| "123".to_string()),
            db_name: env::var("DB_NAME").unwrap_or_else(|_| "bigpicture".to_string()),
            
            // Server
            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "5500".to_string())
                .parse()
                .unwrap_or(5500),
            
            // Image Processing
            thumbnail_max_width: env::var("THUMBNAIL_MAX_WIDTH")
                .unwrap_or_else(|_| "800".to_string())
                .parse()
                .unwrap_or(800),
            thumbnail_max_height: env::var("THUMBNAIL_MAX_HEIGHT")
                .unwrap_or_else(|_| "800".to_string())
                .parse()
                .unwrap_or(800),
            thumbnail_quality: env::var("THUMBNAIL_QUALITY")
                .unwrap_or_else(|_| "80".to_string())
                .parse()
                .unwrap_or(80),
            map_max_width: env::var("MAP_MAX_WIDTH")
                .unwrap_or_else(|_| "800".to_string())
                .parse()
                .unwrap_or(800),
            map_max_height: env::var("MAP_MAX_HEIGHT")
                .unwrap_or_else(|_| "600".to_string())
                .parse()
                .unwrap_or(600),
            map_quality: env::var("MAP_QUALITY")
                .unwrap_or_else(|_| "85".to_string())
                .parse()
                .unwrap_or(85),
            
            // File Upload
            max_file_size_mb: env::var("MAX_FILE_SIZE_MB")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30.0),
            upload_dir: env::var("UPLOAD_DIR").unwrap_or_else(|_| "/uploads".to_string()),
            file_server_url: env::var("FILE_SERVER_URL").unwrap_or_else(|_| "http://localhost:5500".to_string()),
            
            // S3
            s3_bucket_name: env::var("S3_BUCKET_NAME").unwrap_or_else(|_| "bigpicture-uploads".to_string()),
            s3_region: env::var("S3_REGION").unwrap_or_else(|_| "ap-northeast-2".to_string()),
            s3_access_key_id: env::var("AWS_ACCESS_KEY_ID").unwrap_or_else(|_| "".to_string()),
            s3_secret_access_key: env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_else(|_| "".to_string()),
            // JWT
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "changemechangemechangeme".to_string()),
            
            // OAuth
            google_client_id: env::var("GOOGLE_CLIENT_ID").unwrap_or_else(|_| "your-google-client-id".to_string()),
            google_client_ids: env::var("GOOGLE_CLIENT_IDS")
                .unwrap_or_else(|_| "".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        })
    }
    
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
    
    pub fn database_url(&self) -> String {
        if let Ok(url) = env::var("DATABASE_URL") {
            url
        } else {
            format!(
                "postgresql://{}:{}@{}:{}/{}",
                self.db_user, self.db_password, self.db_host, self.db_port, self.db_name
            )
        }
    }
    
    pub fn get_file_url(&self, filename: &str) -> String {
        format!("{}/api/images/download/{}", self.file_server_url, filename)
    }
    
    pub fn get_original_file_url(&self, filename: &str) -> String {
        format!("{}/api/images/download/original/{}", self.file_server_url, filename)
    }
    
    pub fn get_upload_path(&self, image_type: &str) -> String {
        if self.upload_dir.starts_with('/') {
            format!("{}/{}", self.upload_dir, image_type)
        } else {
            format!("{}/{}", self.upload_dir, image_type)
        }
    }
    
    pub fn get_original_upload_path(&self, image_type: &str) -> String {
        if self.upload_dir.starts_with('/') {
            format!("{}/{}_original", self.upload_dir, image_type)
        } else {
            format!("{}/{}_original", self.upload_dir, image_type)
        }
    }
} 