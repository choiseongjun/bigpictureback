use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
use anyhow::Result;
use crate::config::Config;
use log::{info, warn, error};
use h3ron::H3Cell;
use h3ron::Index;
use geo_types::Point;
use rayon::prelude::*;

struct MarkerClusterInfo {
    id: i32,
    member_id: i64,
    latitude: f64,
    longitude: f64,
    emotion_tag: String,
    description: String,
    likes: i32,
    dislikes: i32,
    views: i32,
    author: String,
    thumbnail_img: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct Database {
    pub pool: PgPool,
}

impl Database {
    pub async fn new(config: &Config) -> Result<Self> {
        let database_url = config.database_url();
        
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;
        
        // 데이터베이스 초기화
        Self::init_database(&pool).await?;
        
        Ok(Self { pool })
    }
    
    async fn init_database(pool: &PgPool) -> Result<()> {
        println!("🔧 데이터베이스 초기화 시작...");
        
        // PostGIS 확장 활성화
        println!("🗺️ PostGIS 확장 활성화 중...");
        sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
            .execute(pool)
            .await?;
        println!("✅ PostGIS 확장 활성화 완료");
        
        // bigpicture 스키마 생성
        println!("📁 bigpicture 스키마 생성 중...");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS bigpicture")
            .execute(pool)
            .await?;
        println!("✅ bigpicture 스키마 생성 완료");
        
        // 기존 테이블 삭제 (새로운 구조로 변경)
        println!("🗑️ 기존 테이블 삭제 중...");
        sqlx::query("DROP TABLE IF EXISTS bigpicture.images CASCADE")
            .execute(pool)
            .await?;
        println!("✅ 기존 테이블 삭제 완료");
        
        // 원본 이미지 테이블 생성
        println!("📋 original_images 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.original_images (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                filename VARCHAR(255) NOT NULL UNIQUE,
                original_filename VARCHAR(255) NOT NULL,
                file_path VARCHAR(500) NOT NULL,
                file_size_mb DECIMAL(10, 6) NOT NULL,
                width INTEGER,
                height INTEGER,
                format VARCHAR(50) NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ original_images 테이블 생성 완료");
        
        // WebP 변환 이미지 테이블 생성
        println!("📋 webp_images 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.webp_images (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                original_id UUID NOT NULL REFERENCES bigpicture.original_images(id) ON DELETE CASCADE,
                filename VARCHAR(255) NOT NULL UNIQUE,
                file_path VARCHAR(500) NOT NULL,
                file_size_mb DECIMAL(10, 6) NOT NULL,
                width INTEGER,
                height INTEGER,
                image_type VARCHAR(50) NOT NULL, -- thumbnail, map
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ webp_images 테이블 생성 완료");
        
        // 인덱스 생성
        println!("🔍 인덱스 생성 중...");
        
        // original_images 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_original_images_filename ON bigpicture.original_images(filename)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_original_images_created_at ON bigpicture.original_images(created_at)")
            .execute(pool)
            .await?;
        
        // webp_images 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_webp_images_filename ON bigpicture.webp_images(filename)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_webp_images_original_id ON bigpicture.webp_images(original_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_webp_images_image_type ON bigpicture.webp_images(image_type)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_webp_images_created_at ON bigpicture.webp_images(created_at)")
            .execute(pool)
            .await?;
        
        // members 테이블 생성 (먼저 생성)
        println!("📋 members 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.members (
                id BIGSERIAL PRIMARY KEY,
                email VARCHAR(255) NOT NULL UNIQUE,
                nickname VARCHAR(100) NOT NULL,
                profile_image_url VARCHAR(500),
                region VARCHAR(100),
                gender VARCHAR(20),
                age INTEGER,
                personality_type VARCHAR(50),
                is_active BOOLEAN DEFAULT true,
                email_verified BOOLEAN DEFAULT false,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                last_login_at TIMESTAMP WITH TIME ZONE
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ members 테이블 생성 완료");
        
        // markers 테이블 생성
        println!("📋 markers 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.markers (
                id SERIAL PRIMARY KEY,
                member_id BIGINT REFERENCES bigpicture.members(id) ON DELETE CASCADE,
                location GEOGRAPHY(POINT, 4326),
                emotion_tag TEXT,
                description TEXT,
                likes INTEGER DEFAULT 0,
                dislikes INTEGER DEFAULT 0,
                views INTEGER DEFAULT 0,
                author TEXT,
                thumbnail_img TEXT, -- 기존 썸네일 필드 유지
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ markers 테이블 생성 완료");
        
        // marker_images 테이블 생성 (마커와 이미지 연결)
        println!("📋 marker_images 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.marker_images (
                id SERIAL PRIMARY KEY,
                marker_id INTEGER NOT NULL REFERENCES bigpicture.markers(id) ON DELETE CASCADE,
                image_type VARCHAR(50) NOT NULL, -- thumbnail, detail, gallery
                image_url VARCHAR(500) NOT NULL,
                image_order INTEGER DEFAULT 0, -- 이미지 순서
                is_primary BOOLEAN DEFAULT false, -- 대표 이미지 여부
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ marker_images 테이블 생성 완료");
        
        // 공간 인덱스 생성 (성능 최적화)
        sqlx::query("CREATE INDEX IF NOT EXISTS markers_location_gist ON bigpicture.markers USING GIST (location)")
            .execute(pool)
            .await?;
        
        // marker_images 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_marker_images_marker_id ON bigpicture.marker_images(marker_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_marker_images_image_type ON bigpicture.marker_images(image_type)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_marker_images_is_primary ON bigpicture.marker_images(is_primary)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_marker_images_order ON bigpicture.marker_images(marker_id, image_order)")
            .execute(pool)
            .await?;
        
        // auth_providers 테이블 생성
        println!("📋 auth_providers 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.auth_providers (
                id BIGSERIAL PRIMARY KEY,
                member_id BIGINT NOT NULL REFERENCES bigpicture.members(id) ON DELETE CASCADE,
                provider_type VARCHAR(50) NOT NULL, -- google, kakao, naver, meta, email
                provider_id VARCHAR(255) NOT NULL,
                provider_email VARCHAR(255),
                password_hash VARCHAR(255),
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                
                UNIQUE(provider_type, provider_id),
                UNIQUE(member_id, provider_type)
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ auth_providers 테이블 생성 완료");
        
        // member_markers 테이블 생성 (마커와 유저 연결)
        println!("📋 member_markers 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.member_markers (
                id BIGSERIAL PRIMARY KEY,
                member_id BIGINT NOT NULL REFERENCES bigpicture.members(id) ON DELETE CASCADE,
                marker_id BIGINT NOT NULL REFERENCES bigpicture.markers(id) ON DELETE CASCADE,
                interaction_type VARCHAR(50) NOT NULL, -- created, liked, disliked, viewed, bookmarked
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                
                UNIQUE(member_id, marker_id, interaction_type)
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ member_markers 테이블 생성 완료");
        

        
        // 인덱스 생성
        println!("🔍 추가 인덱스 생성 중...");
        
        // members 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_members_email ON bigpicture.members(email)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_members_nickname ON bigpicture.members(nickname)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_members_created_at ON bigpicture.members(created_at)")
            .execute(pool)
            .await?;
        
        // auth_providers 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_auth_providers_member_id ON bigpicture.auth_providers(member_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_auth_providers_provider_type_id ON bigpicture.auth_providers(provider_type, provider_id)")
            .execute(pool)
            .await?;
        
        // member_markers 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_member_markers_member_id ON bigpicture.member_markers(member_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_member_markers_marker_id ON bigpicture.member_markers(marker_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_member_markers_interaction_type ON bigpicture.member_markers(interaction_type)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_member_markers_member_marker ON bigpicture.member_markers(member_id, marker_id)")
            .execute(pool)
            .await?;
        
        // markers member_id 인덱스
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_member_id ON bigpicture.markers(member_id)")
            .execute(pool)
            .await?;
        
        println!("✅ 인덱스 생성 완료");
        
        // 테이블 존재 확인
        println!("🔍 테이블 존재 확인 중...");
        let original_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'bigpicture' AND table_name = 'original_images')"
        )
        .fetch_one(pool)
        .await?;
        
        let webp_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'bigpicture' AND table_name = 'webp_images')"
        )
        .fetch_one(pool)
        .await?;
        
        let markers_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'bigpicture' AND table_name = 'markers')"
        )
        .fetch_one(pool)
        .await?;
        
        let members_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'bigpicture' AND table_name = 'members')"
        )
        .fetch_one(pool)
        .await?;
        
        let auth_providers_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'bigpicture' AND table_name = 'auth_providers')"
        )
        .fetch_one(pool)
        .await?;
        
        let member_markers_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'bigpicture' AND table_name = 'member_markers')"
        )
        .fetch_one(pool)
        .await?;
        
        if original_exists && webp_exists && markers_exists && members_exists && auth_providers_exists && member_markers_exists {
            println!("✅ 새로운 테이블 구조가 성공적으로 생성되었습니다!");
            
            // 테이블 구조 확인
            println!("📊 original_images 테이블 구조:");
            let original_columns = sqlx::query(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = 'bigpicture' AND table_name = 'original_images' ORDER BY ordinal_position"
            )
            .fetch_all(pool)
            .await?;
            
            for row in original_columns {
                let column_name: String = row.get(0);
                let data_type: String = row.get(1);
                println!("  - {}: {}", column_name, data_type);
            }
            
            println!("📊 webp_images 테이블 구조:");
            let webp_columns = sqlx::query(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = 'bigpicture' AND table_name = 'webp_images' ORDER BY ordinal_position"
            )
            .fetch_all(pool)
            .await?;
            
            for row in webp_columns {
                let column_name: String = row.get(0);
                let data_type: String = row.get(1);
                println!("  - {}: {}", column_name, data_type);
            }
            
            println!("📊 markers 테이블 구조:");
            let markers_columns = sqlx::query(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = 'bigpicture' AND table_name = 'markers' ORDER BY ordinal_position"
            )
            .fetch_all(pool)
            .await?;
            
            for row in markers_columns {
                let column_name: String = row.get(0);
                let data_type: String = row.get(1);
                println!("  - {}: {}", column_name, data_type);
            }
        } else {
            println!("❌ 테이블 생성에 실패했습니다!");
        }
        
        // 회원/멤버 관련 테이블 생성
        println!("📋 members 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.members (
                id SERIAL PRIMARY KEY,
                email VARCHAR(255) UNIQUE NOT NULL,
                nickname VARCHAR(100) NOT NULL,
                profile_image_url TEXT,
                region VARCHAR(100),
                gender VARCHAR(10) CHECK (gender IN ('male', 'female', 'other', 'prefer_not_to_say')),
                age INTEGER CHECK (age IS NULL OR (age >= 1900 AND age <= 2100)),
                personality_type VARCHAR(50),
                is_active BOOLEAN DEFAULT TRUE,
                email_verified BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                last_login_at TIMESTAMP WITH TIME ZONE
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ members 테이블 생성 완료");

        println!("📋 auth_providers 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.auth_providers (
                id SERIAL PRIMARY KEY,
                member_id INTEGER NOT NULL REFERENCES bigpicture.members(id) ON DELETE CASCADE,
                provider_type VARCHAR(20) NOT NULL CHECK (provider_type IN ('email', 'google', 'meta', 'kakao', 'naver')),
                provider_id VARCHAR(255) NOT NULL,
                provider_email VARCHAR(255),
                password_hash VARCHAR(255),
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(provider_type, provider_id)
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ auth_providers 테이블 생성 완료");

        println!("📋 hobbies 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.hobbies (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100) NOT NULL UNIQUE,
                category VARCHAR(50),
                description TEXT,
                is_active BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ hobbies 테이블 생성 완료");

        println!("📋 interests 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.interests (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100) NOT NULL UNIQUE,
                category VARCHAR(50),
                description TEXT,
                is_active BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ interests 테이블 생성 완료");

        println!("📋 member_hobbies 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.member_hobbies (
                id SERIAL PRIMARY KEY,
                member_id INTEGER NOT NULL REFERENCES bigpicture.members(id) ON DELETE CASCADE,
                hobby_id INTEGER NOT NULL REFERENCES bigpicture.hobbies(id) ON DELETE CASCADE,
                proficiency_level INTEGER CHECK (proficiency_level >= 1 AND proficiency_level <= 5),
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(member_id, hobby_id)
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ member_hobbies 테이블 생성 완료");

        println!("📋 member_interests 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.member_interests (
                id SERIAL PRIMARY KEY,
                member_id INTEGER NOT NULL REFERENCES bigpicture.members(id) ON DELETE CASCADE,
                interest_id INTEGER NOT NULL REFERENCES bigpicture.interests(id) ON DELETE CASCADE,
                interest_level INTEGER CHECK (interest_level >= 1 AND interest_level <= 5),
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(member_id, interest_id)
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ member_interests 테이블 생성 완료");
        
        Ok(())
    }
    
    pub async fn save_original_image(
        &self,
        filename: &str,
        original_filename: &str,
        file_path: &str,
        file_size_mb: f64,
        width: Option<u32>,
        height: Option<u32>,
        format: &str,
    ) -> Result<uuid::Uuid> {
        let id = uuid::Uuid::new_v4();
        
        sqlx::query(
            r#"
            INSERT INTO bigpicture.original_images 
            (id, filename, original_filename, file_path, file_size_mb, width, height, format)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#
        )
        .bind(id)
        .bind(filename)
        .bind(original_filename)
        .bind(file_path)
        .bind(file_size_mb)
        .bind(width.map(|w| w as i32))
        .bind(height.map(|h| h as i32))
        .bind(format)
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn save_webp_image(
        &self,
        original_id: uuid::Uuid,
        filename: &str,
        file_path: &str,
        file_size_mb: f64,
        width: Option<u32>,
        height: Option<u32>,
        image_type: &str,
    ) -> Result<uuid::Uuid> {
        let id = uuid::Uuid::new_v4();
        
        sqlx::query(
            r#"
            INSERT INTO bigpicture.webp_images 
            (id, original_id, filename, file_path, file_size_mb, width, height, image_type)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#
        )
        .bind(id)
        .bind(original_id)
        .bind(filename)
        .bind(file_path)
        .bind(file_size_mb)
        .bind(width.map(|w| w as i32))
        .bind(height.map(|h| h as i32))
        .bind(image_type)
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    // 기존 메서드는 호환성을 위해 유지
    pub async fn save_image_info(
        &self,
        filename: &str,
        original_filename: &str,
        file_path: &str,
        file_size_mb: f64,
        width: Option<u32>,
        height: Option<u32>,
        format: &str,
        image_type: &str,
    ) -> Result<uuid::Uuid> {
        let id = uuid::Uuid::new_v4();
        
        sqlx::query(
            r#"
            INSERT INTO bigpicture.images 
            (id, filename, original_filename, file_path, file_size_mb, width, height, format, image_type)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#
        )
        .bind(id)
        .bind(filename)
        .bind(original_filename)
        .bind(file_path)
        .bind(file_size_mb)
        .bind(width.map(|w| w as i32))
        .bind(height.map(|h| h as i32))
        .bind(format)
        .bind(image_type)
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_original_image(&self, filename: &str) -> Result<Option<OriginalImage>> {
        let row = sqlx::query_as::<_, OriginalImage>(
            r#"
            SELECT id, filename, original_filename, file_path, file_size_mb, 
                   width, height, format, created_at, updated_at
            FROM bigpicture.original_images 
            WHERE filename = $1
            "#
        )
        .bind(filename)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn get_webp_image(&self, filename: &str) -> Result<Option<WebpImage>> {
        let row = sqlx::query_as::<_, WebpImage>(
            r#"
            SELECT id, original_id, filename, file_path, file_size_mb, 
                   width, height, image_type, created_at, updated_at
            FROM bigpicture.webp_images 
            WHERE filename = $1
            "#
        )
        .bind(filename)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn get_webp_images_by_original(&self, original_id: uuid::Uuid) -> Result<Vec<WebpImage>> {
        let rows = sqlx::query_as::<_, WebpImage>(
            r#"
            SELECT id, original_id, filename, file_path, file_size_mb, 
                   width, height, image_type, created_at, updated_at
            FROM bigpicture.webp_images 
            WHERE original_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(original_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    pub async fn get_webp_images_by_type(&self, image_type: &str) -> Result<Vec<WebpImage>> {
        let rows = sqlx::query_as::<_, WebpImage>(
            r#"
            SELECT id, original_id, filename, file_path, file_size_mb, 
                   width, height, image_type, created_at, updated_at
            FROM bigpicture.webp_images 
            WHERE image_type = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(image_type)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    // 기존 메서드는 호환성을 위해 유지
    pub async fn get_image_info(&self, filename: &str) -> Result<Option<ImageInfo>> {
        let row = sqlx::query_as::<_, ImageInfo>(
            r#"
            SELECT id, filename, original_filename, file_path, file_size_mb, 
                   width, height, format, image_type, created_at, updated_at
            FROM bigpicture.images 
            WHERE filename = $1
            "#
        )
        .bind(filename)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn get_images_by_type(&self, image_type: &str) -> Result<Vec<ImageInfo>> {
        let rows = sqlx::query_as::<_, ImageInfo>(
            r#"
            SELECT id, filename, original_filename, file_path, file_size_mb, 
                   width, height, format, image_type, created_at, updated_at
            FROM bigpicture.images 
            WHERE image_type = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(image_type)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    pub async fn delete_image(&self, filename: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM bigpicture.images WHERE filename = $1")
            .bind(filename)
            .execute(&self.pool)
            .await?;
        
        Ok(result.rows_affected() > 0)
    }
    
    pub async fn get_total_size_mb(&self, image_type: Option<&str>) -> Result<f64> {
        let query = if let Some(img_type) = image_type {
            sqlx::query("SELECT COALESCE(SUM(file_size_mb), 0) FROM bigpicture.images WHERE image_type = $1")
                .bind(img_type)
        } else {
            sqlx::query("SELECT COALESCE(SUM(file_size_mb), 0) FROM bigpicture.images")
        };
        
        let total_size: f64 = query.fetch_one(&self.pool).await?.get(0);
        Ok(total_size)
    }
    
    pub async fn get_markers(
        &self,
        lat: f64,
        lng: f64,
        lat_delta: f64,
        lng_delta: f64,
        emotion_tags: Option<Vec<String>>,
        min_likes: Option<i32>,
        min_views: Option<i32>,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        limit: Option<i32>,
        user_id: Option<i64>, // 추가: 내 마커만 조회
    ) -> Result<Vec<Marker>> {
        info!("🗄️ 데이터베이스 쿼리 시작:");
        
        let lat_min = lat - lat_delta / 2.0;
        let lat_max = lat + lat_delta / 2.0;
        let lng_min = lng - lng_delta / 2.0;
        let lng_max = lng + lng_delta / 2.0;
        
        info!("   - 검색 범위: lat({} ~ {}), lng({} ~ {})", lat_min, lat_max, lng_min, lng_max);
        
        // 정렬 동적 처리
        let allowed_sort = ["created_at", "likes", "views", "dislikes"];
        let sort_col = sort_by.filter(|s| allowed_sort.contains(&s.to_lowercase().as_str())).unwrap_or("created_at");
        let order = sort_order.filter(|o| o.eq_ignore_ascii_case("asc") || o.eq_ignore_ascii_case("desc")).unwrap_or("desc");
        let mut query = format!(
            "SELECT id, member_id, ST_AsText(location) as location, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, created_at, updated_at
             FROM bigpicture.markers 
             WHERE ST_Within(location::geometry, ST_MakeEnvelope({}, {}, {}, {}, 4326))",
            lng_min, lat_min, lng_max, lat_max
        );
        
        // 내 마커만 조회
        if let Some(uid) = user_id {
            query.push_str(&format!(" AND member_id = {}", uid));
            info!("   - 내 마커만 필터: member_id = {}", uid);
        }
        
        // 감성 태그 필터
        if let Some(tags) = emotion_tags {
            if !tags.is_empty() {
                let tags_str = tags.iter().map(|tag| format!("'{}'", tag)).collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND emotion_tag IN ({})", tags_str));
                info!("   - 감성 태그 필터: {}", tags_str);
            }
        }
        
        // 최소 좋아요 수 필터
        if let Some(likes) = min_likes {
            query.push_str(&format!(" AND likes >= {}", likes));
            info!("   - 최소 좋아요: {}", likes);
        }
        
        // 최소 조회수 필터
        if let Some(views) = min_views {
            query.push_str(&format!(" AND views >= {}", views));
            info!("   - 최소 조회수: {}", views);
        }
        
        query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
        
        // LIMIT 추가 (기본값 1000개)
        let limit_value = limit.unwrap_or(5000);
        query.push_str(&format!(" LIMIT {}", limit_value));
        
        info!("   - 최종 SQL 쿼리: {}", query);
        
        // 쿼리 실행
        let markers = sqlx::query_as::<_, Marker>(&query)
            .fetch_all(&self.pool)
            .await?;
        
        info!("   - 쿼리 실행 완료: {}개 결과", markers.len());
        
        Ok(markers)
    }

    /// 피드용 마커 조회 (시간순 내림차순, 페이지네이션 지원)
    pub async fn get_markers_feed(
        &self,
        page: i32,
        limit: i32,
        emotion_tags: Option<Vec<String>>,
        min_likes: Option<i32>,
        min_views: Option<i32>,
        user_id: Option<i64>,
    ) -> Result<(Vec<Marker>, i64)> { // (마커 목록, 전체 개수)
        info!("🗄️ 피드 마커 조회 시작:");
        info!("   - 페이지: {}, 제한: {}", page, limit);
        
        let offset = (page - 1) * limit;
        
        let mut where_conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();
        let mut param_count = 1;
        
        // 특정 사용자 마커만 조회
        if let Some(uid) = user_id {
            where_conditions.push(format!("member_id = ${}", param_count));
            params.push(uid.to_string());
            param_count += 1;
            info!("   - 사용자 필터: member_id = {}", uid);
        }
        
        // 감성 태그 필터
        if let Some(tags) = emotion_tags {
            if !tags.is_empty() {
                let tag_conditions: Vec<String> = tags.iter()
                    .map(|tag| format!("emotion_tag LIKE '%{}%'", tag))
                    .collect();
                where_conditions.push(format!("({})", tag_conditions.join(" OR ")));
                info!("   - 감성 태그 필터: {:?}", tags);
            }
        }
        
        // 최소 좋아요 수 필터
        if let Some(min_likes) = min_likes {
            where_conditions.push(format!("likes >= ${}", param_count));
            params.push(min_likes.to_string());
            param_count += 1;
            info!("   - 최소 좋아요 수: {}", min_likes);
        }
        
        // 최소 조회수 필터
        if let Some(min_views) = min_views {
            where_conditions.push(format!("views >= ${}", param_count));
            params.push(min_views.to_string());
            param_count += 1;
            info!("   - 최소 조회수: {}", min_views);
        }
        
        let where_clause = if where_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_conditions.join(" AND "))
        };
        
        // 전체 개수 조회
        let count_query = format!(
            "SELECT COUNT(*) as total FROM bigpicture.markers {}",
            where_clause
        );
        
        let total_count: i64 = if params.is_empty() {
            sqlx::query_scalar(&count_query)
                .fetch_one(&self.pool)
                .await?
        } else {
            let mut query_builder = sqlx::query_scalar(&count_query);
            for param in &params {
                query_builder = query_builder.bind(param);
            }
            query_builder.fetch_one(&self.pool).await?
        };
        
        // 마커 목록 조회
        let markers_query = format!(
            "SELECT id, member_id, ST_AsText(location) as location, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, created_at, updated_at
             FROM bigpicture.markers 
             {} 
             ORDER BY created_at DESC 
             LIMIT {} OFFSET {}",
            where_clause, limit, offset
        );
        
        let markers = if params.is_empty() {
            sqlx::query_as::<_, Marker>(&markers_query)
                .fetch_all(&self.pool)
                .await?
        } else {
            let mut query_builder = sqlx::query_as::<_, Marker>(&markers_query);
            for param in &params {
                query_builder = query_builder.bind(param);
            }
            query_builder.fetch_all(&self.pool).await?
        };
        
        info!("✅ 피드 쿼리 완료: {}개 마커 반환 (전체: {}개)", markers.len(), total_count);
        Ok((markers, total_count))
    }

    // 마커 이미지 관련 함수들
    pub async fn add_marker_image(
        &self,
        marker_id: i32,
        image_type: &str,
        image_url: &str,
        image_order: i32,
        is_primary: bool,
    ) -> Result<i32> {
        let rec = sqlx::query(
            r#"
            INSERT INTO bigpicture.marker_images
                (marker_id, image_type, image_url, image_order, is_primary)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(marker_id)
        .bind(image_type)
        .bind(image_url)
        .bind(image_order)
        .bind(is_primary)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(rec.get("id"))
    }

    pub async fn get_marker_images(&self, marker_id: i32) -> Result<Vec<MarkerImage>> {
        let rows = sqlx::query_as::<_, MarkerImage>(
            r#"
            SELECT id, marker_id, image_type, image_url, image_order, is_primary, created_at, updated_at
            FROM bigpicture.marker_images 
            WHERE marker_id = $1
            ORDER BY image_order ASC, created_at ASC
            "#
        )
        .bind(marker_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }

    pub async fn get_marker_images_by_type(&self, marker_id: i32, image_type: &str) -> Result<Vec<MarkerImage>> {
        let rows = sqlx::query_as::<_, MarkerImage>(
            r#"
            SELECT id, marker_id, image_type, image_url, image_order, is_primary, created_at, updated_at
            FROM bigpicture.marker_images 
            WHERE marker_id = $1 AND image_type = $2
            ORDER BY image_order ASC, created_at ASC
            "#
        )
        .bind(marker_id)
        .bind(image_type)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }

    pub async fn get_marker_primary_image(&self, marker_id: i32) -> Result<Option<MarkerImage>> {
        let row = sqlx::query_as::<_, MarkerImage>(
            r#"
            SELECT id, marker_id, image_type, image_url, image_order, is_primary, created_at, updated_at
            FROM bigpicture.marker_images 
            WHERE marker_id = $1 AND is_primary = true
            LIMIT 1
            "#
        )
        .bind(marker_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }

    pub async fn update_marker_image_order(&self, image_id: i32, new_order: i32) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE bigpicture.marker_images
            SET image_order = $1, updated_at = NOW()
            WHERE id = $2
            "#
        )
        .bind(new_order)
        .bind(image_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    pub async fn set_marker_primary_image(&self, marker_id: i32, image_id: i32) -> Result<()> {
        // 먼저 모든 이미지의 is_primary를 false로 설정
        sqlx::query(
            r#"
            UPDATE bigpicture.marker_images
            SET is_primary = false, updated_at = NOW()
            WHERE marker_id = $1
            "#
        )
        .bind(marker_id)
        .execute(&self.pool)
        .await?;
        
        // 지정된 이미지를 primary로 설정
        sqlx::query(
            r#"
            UPDATE bigpicture.marker_images
            SET is_primary = true, updated_at = NOW()
            WHERE id = $1 AND marker_id = $2
            "#
        )
        .bind(image_id)
        .bind(marker_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    pub async fn delete_marker_image(&self, image_id: i32) -> Result<bool> {
        let result = sqlx::query("DELETE FROM bigpicture.marker_images WHERE id = $1")
            .bind(image_id)
            .execute(&self.pool)
            .await?;
        
        Ok(result.rows_affected() > 0)
    }

    /// 회원 등록
    pub async fn create_member(
        &self,
        email: &str,
        nickname: &str,
        profile_image_url: Option<&str>,
        region: Option<&str>,
        gender: Option<&str>,
        birth_year: Option<i32>,
        personality_type: Option<&str>,
    ) -> Result<Member> {
        let rec = sqlx::query_as::<_, Member>(
            r#"
            INSERT INTO bigpicture.members
                (email, nickname, profile_image_url, region, gender, age, personality_type)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#
        )
        .bind(email)
        .bind(nickname)
        .bind(profile_image_url)
        .bind(region)
        .bind(gender)
        .bind(birth_year)
        .bind(personality_type)
        .fetch_one(&self.pool)
        .await?;
        Ok(rec)
    }

    /// 회원 조회 by id
    pub async fn get_member_by_id(&self, id: i64) -> Result<Option<Member>> {
        let rec = sqlx::query_as::<_, Member>(
            r#"
            SELECT * FROM bigpicture.members WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec)
    }

    /// 회원 조회 by id (마커 정보 포함)
    pub async fn get_member_with_markers(&self, id: i64) -> Result<Option<(Member, Vec<MemberMarker>)>> {
        // 회원 정보 조회
        let member = match self.get_member_by_id(id).await? {
            Some(member) => member,
            None => return Ok(None),
        };
        
        // 회원의 마커 상호작용 조회
        let markers = self.get_member_marker_interactions(id).await?;
        
        Ok(Some((member, markers)))
    }

    /// 회원 조회 by id (마커 상세 정보 포함)
    pub async fn get_member_with_marker_details(&self, id: i64) -> Result<Option<(Member, Vec<(MemberMarker, Marker)>)>> {
        // 회원 정보 조회
        let member = match self.get_member_by_id(id).await? {
            Some(member) => member,
            None => return Ok(None),
        };
        
        // 회원의 마커 상세 정보 조회
        let marker_details = self.get_member_markers_with_details(id).await?;
        
        Ok(Some((member, marker_details)))
    }

    /// 회원 조회 by id (마커 통계 포함)
    pub async fn get_member_with_stats(&self, id: i64) -> Result<Option<(Member, serde_json::Value)>> {
        // 회원 정보 조회
        let member = match self.get_member_by_id(id).await? {
            Some(member) => member,
            None => return Ok(None),
        };
        
        // 회원의 마커 통계 조회
        let stats = self.get_member_marker_stats(id).await?;
        
        Ok(Some((member, stats)))
    }

    /// 회원 조회 by email
    pub async fn get_member_by_email(&self, email: &str) -> Result<Option<Member>> {
        let rec = sqlx::query_as::<_, Member>(
            r#"
            SELECT * FROM bigpicture.members WHERE email = $1
            "#
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec)
    }

    /// 전체 회원 목록 (limit 옵션)
    pub async fn list_members(&self, limit: Option<i64>) -> Result<Vec<Member>> {
        let recs = sqlx::query_as::<_, Member>(
            r#"
            SELECT * FROM bigpicture.members ORDER BY id DESC LIMIT $1
            "#
        )
        .bind(limit.unwrap_or(100))
        .fetch_all(&self.pool)
        .await?;
        Ok(recs)
    }

    /// 소셜 로그인 회원 생성 (트랜잭션으로 처리)
    pub async fn create_social_member(
        &self,
        email: &str,
        nickname: &str,
        provider_type: &str,
        provider_id: &str,
        provider_email: Option<&str>,
        profile_image_url: Option<&str>,
        region: Option<&str>,
        gender: Option<&str>,
        birth_year: Option<i32>,
        personality_type: Option<&str>,
    ) -> Result<(Member, AuthProvider)> {
        let mut tx = self.pool.begin().await?;
        
        // 1. 회원 생성
        let member = sqlx::query_as::<_, Member>(
            r#"
            INSERT INTO bigpicture.members
                (email, nickname, profile_image_url, region, gender, age, personality_type, email_verified)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#
        )
        .bind(email)
        .bind(nickname)
        .bind(profile_image_url)
        .bind(region)
        .bind(gender)
        .bind(birth_year)
        .bind(personality_type)
        .bind(provider_type != "email") // 소셜 로그인은 이메일 인증 완료로 간주
        .fetch_one(&mut *tx)
        .await?;

        // 2. 인증 제공자 정보 생성
        let auth_provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            INSERT INTO bigpicture.auth_providers
                (member_id, provider_type, provider_id, provider_email)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#
        )
        .bind(member.id)
        .bind(provider_type)
        .bind(provider_id)
        .bind(provider_email)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((member, auth_provider))
    }

    /// 이메일/비밀번호 회원 생성
    pub async fn create_email_member(
        &self,
        email: &str,
        nickname: &str,
        password_hash: &str,
        profile_image_url: Option<&str>,
        region: Option<&str>,
        gender: Option<&str>,
        birth_year: Option<i32>,
        personality_type: Option<&str>,
    ) -> Result<(Member, AuthProvider)> {
        let mut tx = self.pool.begin().await?;
        
        // 1. 회원 생성
        let member = sqlx::query_as::<_, Member>(
            r#"
            INSERT INTO bigpicture.members
                (email, nickname, profile_image_url, region, gender, age, personality_type, email_verified)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#
        )
        .bind(email)
        .bind(nickname)
        .bind(profile_image_url)
        .bind(region)
        .bind(gender)
        .bind(birth_year)
        .bind(personality_type)
        .bind(false) // 이메일 인증 필요
        .fetch_one(&mut *tx)
        .await?;

        // 2. 인증 제공자 정보 생성
        let auth_provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            INSERT INTO bigpicture.auth_providers
                (member_id, provider_type, provider_id, provider_email, password_hash)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#
        )
        .bind(member.id)
        .bind("email")
        .bind(email) // 이메일을 provider_id로 사용
        .bind(email)
        .bind(password_hash)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((member, auth_provider))
    }

    /// 소셜 로그인으로 기존 회원 찾기
    pub async fn find_member_by_social_provider(
        &self,
        provider_type: &str,
        provider_id: &str,
    ) -> Result<Option<(Member, AuthProvider)>> {
        // 먼저 auth_provider로 member_id 찾기
        let auth_provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            SELECT * FROM bigpicture.auth_providers 
            WHERE provider_type = $1 AND provider_id = $2
            "#
        )
        .bind(provider_type)
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(auth) = auth_provider {
            // member_id로 회원 정보 찾기
            let member = sqlx::query_as::<_, Member>(
                r#"
                SELECT * FROM bigpicture.members 
                WHERE id = $1
                "#
            )
            .bind(auth.member_id)
            .fetch_optional(&self.pool)
            .await?;
            
            if let Some(m) = member {
                Ok(Some((m, auth)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// 이메일로 기존 회원 찾기
    pub async fn find_member_by_email(
        &self,
        email: &str,
    ) -> Result<Option<(Member, AuthProvider)>> {
        // 먼저 이메일로 회원 찾기
        let member = sqlx::query_as::<_, Member>(
            r#"
            SELECT * FROM bigpicture.members 
            WHERE email = $1
            "#
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(m) = member {
            // member_id로 auth_provider 찾기
            let auth_provider = sqlx::query_as::<_, AuthProvider>(
                r#"
                SELECT * FROM bigpicture.auth_providers 
                WHERE member_id = $1
                LIMIT 1
                "#
            )
            .bind(m.id)
            .fetch_optional(&self.pool)
            .await?;
            
            if let Some(auth) = auth_provider {
                Ok(Some((m, auth)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// 회원의 마지막 로그인 시간 업데이트
    pub async fn update_last_login(&self, member_id: i64) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE bigpicture.members 
            SET last_login_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#
        )
        .bind(member_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    /// 회원에게 추가 소셜 로그인 연결
    pub async fn link_social_provider(
        &self,
        member_id: i64,
        provider_type: &str,
        provider_id: &str,
        provider_email: Option<&str>,
    ) -> Result<AuthProvider> {
        let auth_provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            INSERT INTO bigpicture.auth_providers
                (member_id, provider_type, provider_id, provider_email)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#
        )
        .bind(member_id)
        .bind(provider_type)
        .bind(provider_id)
        .bind(provider_email)
        .fetch_one(&self.pool)
        .await?;

        Ok(auth_provider)
    }

    // 관심사 연결
    pub async fn add_member_interests(&self, member_id: i64, interests: &[String]) -> Result<()> {
        for interest_name in interests {
            // 관심사 id 찾기 또는 생성
            let interest = sqlx::query_as::<_, Interest>(
                r#"
                INSERT INTO bigpicture.interests (name, is_active)
                VALUES ($1, true)
                ON CONFLICT (name) DO UPDATE SET is_active = true
                RETURNING *
                "#
            )
            .bind(interest_name)
            .fetch_one(&self.pool)
            .await?;
            // 연결
            sqlx::query(
                r#"
                INSERT INTO bigpicture.member_interests (member_id, interest_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#
            )
            .bind(member_id)
            .bind(interest.id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
    // 취미 연결
    pub async fn add_member_hobbies(&self, member_id: i64, hobbies: &[String]) -> Result<()> {
        for hobby_name in hobbies {
            // 취미 id 찾기 또는 생성
            let hobby = sqlx::query_as::<_, Hobby>(
                r#"
                INSERT INTO bigpicture.hobbies (name, is_active)
                VALUES ($1, true)
                ON CONFLICT (name) DO UPDATE SET is_active = true
                RETURNING *
                "#
            )
            .bind(hobby_name)
            .fetch_one(&self.pool)
            .await?;
            // 연결
            sqlx::query(
                r#"
                INSERT INTO bigpicture.member_hobbies (member_id, hobby_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#
            )
            .bind(member_id)
            .bind(hobby.id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// 마커 생성
    pub async fn create_marker(
        &self,
        member_id: i64,
        latitude: f64,
        longitude: f64,
        emotion_tag: &str,
        description: &str,
        author: &str,
        thumbnail_img: Option<&str>,
    ) -> Result<Marker> {
        let marker = sqlx::query_as::<_, Marker>(
            r#"
            INSERT INTO bigpicture.markers
                (member_id, location, emotion_tag, description, author, thumbnail_img)
            VALUES ($1, ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4, $5, $6, $7)
            RETURNING id, member_id, ST_AsText(location) as location, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, created_at, updated_at
            "#
        )
        .bind(member_id)
        .bind(longitude) // PostGIS는 (longitude, latitude) 순서
        .bind(latitude)
        .bind(emotion_tag)
        .bind(description)
        .bind(author)
        .bind(thumbnail_img)
        .fetch_one(&self.pool)
        .await?;

        Ok(marker)
    }

    /// 마커 좋아요/싫어요 처리
    pub async fn toggle_marker_reaction(
        &self,
        member_id: i64,
        marker_id: i64,
        reaction_type: &str, // "like" 또는 "dislike"
    ) -> Result<(i32, i32)> { // (좋아요 수, 싫어요 수) 반환
        let mut tx = self.pool.begin().await?;
        
        // 기존 반응 확인
        let existing = sqlx::query_as::<_, MemberMarker>(
            r#"
            SELECT * FROM bigpicture.member_markers 
            WHERE member_id = $1 AND marker_id = $2 AND interaction_type IN ('liked', 'disliked')
            "#
        )
        .bind(member_id)
        .bind(marker_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(existing_reaction) = existing {
            if existing_reaction.interaction_type == reaction_type {
                // 같은 반응이면 제거
                sqlx::query(
                    "DELETE FROM bigpicture.member_markers WHERE id = $1"
                )
                .bind(existing_reaction.id)
                .execute(&mut *tx)
                .await?;

                // 마커 카운트 감소
                let update_query = match reaction_type {
                    "liked" => "UPDATE bigpicture.markers SET likes = GREATEST(likes - 1, 0) WHERE id = $1",
                    "disliked" => "UPDATE bigpicture.markers SET dislikes = GREATEST(dislikes - 1, 0) WHERE id = $1",
                    _ => return Err(anyhow::anyhow!("Invalid reaction type")),
                };
                sqlx::query(update_query)
                    .bind(marker_id)
                    .execute(&mut *tx)
                    .await?;
            } else {
                // 다른 반응이면 변경
                sqlx::query(
                    "UPDATE bigpicture.member_markers SET interaction_type = $1, updated_at = NOW() WHERE id = $2"
                )
                .bind(reaction_type)
                .bind(existing_reaction.id)
                .execute(&mut *tx)
                .await?;

                // 마커 카운트 업데이트
                if reaction_type == "liked" {
                    sqlx::query(
                        "UPDATE bigpicture.markers SET likes = likes + 1, dislikes = GREATEST(dislikes - 1, 0) WHERE id = $1"
                    )
                    .bind(marker_id)
                    .execute(&mut *tx)
                    .await?;
                } else {
                    sqlx::query(
                        "UPDATE bigpicture.markers SET dislikes = dislikes + 1, likes = GREATEST(likes - 1, 0) WHERE id = $1"
                    )
                    .bind(marker_id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        } else {
            // 새로운 반응 추가
            sqlx::query(
                r#"
                INSERT INTO bigpicture.member_markers
                    (member_id, marker_id, interaction_type)
                VALUES ($1, $2, $3)
                "#
            )
            .bind(member_id)
            .bind(marker_id)
            .bind(reaction_type)
            .execute(&mut *tx)
            .await?;

            // 마커 카운트 증가
            let update_query = match reaction_type {
                "liked" => "UPDATE bigpicture.markers SET likes = likes + 1 WHERE id = $1",
                "disliked" => "UPDATE bigpicture.markers SET dislikes = dislikes + 1 WHERE id = $1",
                _ => return Err(anyhow::anyhow!("Invalid reaction type")),
            };
            sqlx::query(update_query)
                .bind(marker_id)
                .execute(&mut *tx)
                .await?;
        }

        // 업데이트된 카운트 조회
        let counts = sqlx::query_as::<_, (i32, i32)>(
            "SELECT likes, dislikes FROM bigpicture.markers WHERE id = $1"
        )
        .bind(marker_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(counts)
    }

    /// 마커 조회 기록 추가
    pub async fn add_marker_view(&self, member_id: i64, marker_id: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        
        // 기존 조회 기록 확인
        let existing = sqlx::query_as::<_, MemberMarker>(
            r#"
            SELECT * FROM bigpicture.member_markers 
            WHERE member_id = $1 AND marker_id = $2 AND interaction_type = 'viewed'
            "#
        )
        .bind(member_id)
        .bind(marker_id)
        .fetch_optional(&mut *tx)
        .await?;

        if existing.is_none() {
            // 새로운 조회 기록 추가
            sqlx::query(
                r#"
                INSERT INTO bigpicture.member_markers
                    (member_id, marker_id, interaction_type)
                VALUES ($1, $2, 'viewed')
                "#
            )
            .bind(member_id)
            .bind(marker_id)
            .execute(&mut *tx)
            .await?;

            // 마커 조회수 증가
            sqlx::query(
                "UPDATE bigpicture.markers SET views = views + 1 WHERE id = $1"
            )
            .bind(marker_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// 마커 북마크 토글
    pub async fn toggle_marker_bookmark(&self, member_id: i64, marker_id: i64) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        
        // 기존 북마크 확인
        let existing = sqlx::query_as::<_, MemberMarker>(
            r#"
            SELECT * FROM bigpicture.member_markers 
            WHERE member_id = $1 AND marker_id = $2 AND interaction_type = 'bookmarked'
            "#
        )
        .bind(member_id)
        .bind(marker_id)
        .fetch_optional(&mut *tx)
        .await?;

        let is_bookmarked = if let Some(existing_bookmark) = existing {
            // 북마크 제거
            sqlx::query(
                "DELETE FROM bigpicture.member_markers WHERE id = $1"
            )
            .bind(existing_bookmark.id)
            .execute(&mut *tx)
            .await?;
            false
        } else {
            // 북마크 추가
            sqlx::query(
                r#"
                INSERT INTO bigpicture.member_markers
                    (member_id, marker_id, interaction_type)
                VALUES ($1, $2, 'bookmarked')
                "#
            )
            .bind(member_id)
            .bind(marker_id)
            .execute(&mut *tx)
            .await?;
            true
        };

        tx.commit().await?;
        Ok(is_bookmarked)
    }

    /// 유저가 생성한 마커 목록 조회
    pub async fn get_member_created_markers(&self, member_id: i64, limit: Option<i32>) -> Result<Vec<Marker>> {
        let markers = sqlx::query_as::<_, Marker>(
            r#"
            SELECT id, ST_AsText(location) as location, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, member_id, created_at, updated_at 
            FROM bigpicture.markers 
            WHERE member_id = $1 
            ORDER BY created_at DESC 
            LIMIT $2
            "#
        )
        .bind(member_id)
        .bind(limit.unwrap_or(50))
        .fetch_all(&self.pool)
        .await?;
        Ok(markers)
    }

    /// 유저가 좋아요한 마커 목록 조회
    pub async fn get_member_liked_markers(&self, member_id: i64, limit: Option<i32>) -> Result<Vec<Marker>> {
        let markers = sqlx::query_as::<_, Marker>(
            r#"
            SELECT m.id, ST_AsText(m.location) as location, m.emotion_tag, m.description, m.likes, m.dislikes, m.views, m.author, m.thumbnail_img, m.member_id, m.created_at, m.updated_at 
            FROM bigpicture.markers m
            INNER JOIN bigpicture.member_markers mm ON m.id = mm.marker_id
            WHERE mm.member_id = $1 AND mm.interaction_type = 'liked'
            ORDER BY mm.created_at DESC 
            LIMIT $2
            "#
        )
        .bind(member_id)
        .bind(limit.unwrap_or(50))
        .fetch_all(&self.pool)
        .await?;
        Ok(markers)
    }

    /// 유저가 북마크한 마커 목록 조회
    pub async fn get_member_bookmarked_markers(&self, member_id: i64, limit: Option<i32>) -> Result<Vec<Marker>> {
        let markers = sqlx::query_as::<_, Marker>(
            r#"
            SELECT m.id, ST_AsText(m.location) as location, m.emotion_tag, m.description, m.likes, m.dislikes, m.views, m.author, m.thumbnail_img, m.member_id, m.created_at, m.updated_at 
            FROM bigpicture.markers m
            INNER JOIN bigpicture.member_markers mm ON m.id = mm.marker_id
            WHERE mm.member_id = $1 AND mm.interaction_type = 'bookmarked'
            ORDER BY mm.created_at DESC 
            LIMIT $2
            "#
        )
        .bind(member_id)
        .bind(limit.unwrap_or(50))
        .fetch_all(&self.pool)
        .await?;
        Ok(markers)
    }

    /// 마커의 상세 정보 조회
    pub async fn get_marker_detail(&self, marker_id: i64) -> Result<Option<Marker>> {
        let marker = sqlx::query_as::<_, Marker>(
            "SELECT id, member_id, ST_AsText(location) as location, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, created_at, updated_at FROM bigpicture.markers WHERE id = $1"
        )
        .bind(marker_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(marker)
    }

    /// 3번 사용자와 마커 연결 (복합키 사용)
    pub async fn connect_member_to_marker(&self, member_id: i64, marker_id: i64, interaction_type: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO bigpicture.member_markers (member_id, marker_id, interaction_type)
            VALUES ($1, $2, $3)
            ON CONFLICT (member_id, marker_id, interaction_type) 
            DO UPDATE SET updated_at = NOW()
            "#
        )
        .bind(member_id)
        .bind(marker_id)
        .bind(interaction_type)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    /// 3번 사용자의 모든 마커 상호작용 조회
    pub async fn get_member_marker_interactions(&self, member_id: i64) -> Result<Vec<MemberMarker>> {
        let recs = sqlx::query_as::<_, MemberMarker>(
            r#"
            SELECT id, member_id, marker_id, interaction_type, created_at, updated_at
            FROM bigpicture.member_markers 
            WHERE member_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(member_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(recs)
    }

    /// 3번 사용자의 특정 상호작용 타입 마커 조회
    pub async fn get_member_markers_by_interaction(&self, member_id: i64, interaction_type: &str) -> Result<Vec<MemberMarker>> {
        let recs = sqlx::query_as::<_, MemberMarker>(
            r#"
            SELECT id, member_id, marker_id, interaction_type, created_at, updated_at
            FROM bigpicture.member_markers 
            WHERE member_id = $1 AND interaction_type = $2
            ORDER BY created_at DESC
            "#
        )
        .bind(member_id)
        .bind(interaction_type)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(recs)
    }

    /// 3번 사용자와 마커 상세 정보 함께 조회 (JOIN)
    pub async fn get_member_markers_with_details(&self, member_id: i64) -> Result<Vec<(MemberMarker, Marker)>> {
        let recs = sqlx::query(
            r#"
            SELECT 
                mm.id as mm_id, mm.member_id, mm.marker_id, mm.interaction_type, 
                mm.created_at as mm_created_at, mm.updated_at as mm_updated_at,
                m.id as m_id, m.member_id, ST_AsText(m.location) as location, m.emotion_tag, 
                m.description, m.likes, m.dislikes, m.views, m.author, m.thumbnail_img,
                m.created_at as m_created_at, m.updated_at as m_updated_at
            FROM bigpicture.member_markers mm
            JOIN bigpicture.markers m ON mm.marker_id = m.id
            WHERE mm.member_id = $1
            ORDER BY mm.created_at DESC
            "#
        )
        .bind(member_id)
        .fetch_all(&self.pool)
        .await?;
        
        let mut result = Vec::new();
        for row in recs {
            let member_marker = MemberMarker {
                id: row.get("mm_id"),
                member_id: row.get("member_id"),
                marker_id: row.get("marker_id"),
                interaction_type: row.get("interaction_type"),
                created_at: row.get("mm_created_at"),
                updated_at: row.get("mm_updated_at"),
            };
            
            let marker = Marker {
                id: row.get("m_id"),
                member_id: row.get("member_id"),
                location: row.get("location"),
                emotion_tag: row.get("emotion_tag"),
                description: row.get("description"),
                likes: row.get("likes"),
                dislikes: row.get("dislikes"),
                views: row.get("views"),
                author: row.get("author"),
                thumbnail_img: row.get("thumbnail_img"),
                created_at: row.get("m_created_at"),
                updated_at: row.get("m_updated_at"),
            };
            
            result.push((member_marker, marker));
        }
        
        Ok(result)
    }

    /// 3번 사용자의 마커 상호작용 통계 조회
    pub async fn get_member_marker_stats(&self, member_id: i64) -> Result<serde_json::Value> {
        let stats = sqlx::query(
            r#"
            SELECT 
                interaction_type,
                COUNT(*) as count,
                MIN(created_at) as first_interaction,
                MAX(created_at) as last_interaction
            FROM bigpicture.member_markers 
            WHERE member_id = $1
            GROUP BY interaction_type
            ORDER BY count DESC
            "#
        )
        .bind(member_id)
        .fetch_all(&self.pool)
        .await?;
        
        let mut result = serde_json::Map::new();
        for row in stats {
            let interaction_type: String = row.get("interaction_type");
            let count: i64 = row.get("count");
            let first_interaction: Option<chrono::DateTime<chrono::Utc>> = row.get("first_interaction");
            let last_interaction: Option<chrono::DateTime<chrono::Utc>> = row.get("last_interaction");
            
            let mut interaction_data = serde_json::Map::new();
            interaction_data.insert("count".to_string(), serde_json::Value::Number(count.into()));
            if let Some(first) = first_interaction {
                interaction_data.insert("first_interaction".to_string(), serde_json::Value::String(first.to_rfc3339()));
            }
            if let Some(last) = last_interaction {
                interaction_data.insert("last_interaction".to_string(), serde_json::Value::String(last.to_rfc3339()));
            }
            
            result.insert(interaction_type, serde_json::Value::Object(interaction_data));
        }
        
        Ok(serde_json::Value::Object(result))
    }

    pub async fn get_markers_cluster(
        &self,
        lat: f64,
        lng: f64,
        lat_delta: f64,
        lng_delta: f64,
        emotion_tags: Option<Vec<String>>,
        min_likes: Option<i32>,
        min_views: Option<i32>,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        limit: Option<i32>,
        user_id: Option<i64>,
    ) -> Result<Vec<serde_json::Value>> {
        // 현재 화면보다 약간 더 넓은 영역을 조회해서 지도 이동 시 미리 로딩
        let buffer_factor = 1.2; // 20% 더 넓은 영역 조회
        let lat_min = lat - (lat_delta / 2.0) * buffer_factor;
        let lat_max = lat + (lat_delta / 2.0) * buffer_factor;
        let lng_min = lng - (lng_delta / 2.0) * buffer_factor;
        let lng_max = lng + (lng_delta / 2.0) * buffer_factor;

        let mut query = format!(
            "SELECT m.id, m.member_id, ST_Y(m.location::geometry) as latitude, ST_X(m.location::geometry) as longitude, 
                    m.emotion_tag, m.description, m.likes, m.dislikes, m.views, m.author, m.thumbnail_img, 
                    m.created_at, m.updated_at
             FROM bigpicture.markers m
             WHERE ST_Within(m.location::geometry, ST_MakeEnvelope({}, {}, {}, {}, 4326))",
            lng_min, lat_min, lng_max, lat_max
        );
        if let Some(uid) = user_id {
            query.push_str(&format!(" AND member_id = {}", uid));
        }
        if let Some(tags) = &emotion_tags {
            if !tags.is_empty() {
                let tags_str = tags.iter().map(|tag| format!("'{}'", tag)).collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND emotion_tag IN ({})", tags_str));
            }
        }
        if let Some(likes) = min_likes {
            query.push_str(&format!(" AND likes >= {}", likes));
        }
        if let Some(views) = min_views {
            query.push_str(&format!(" AND views >= {}", views));
        }
        query.push_str(" ORDER BY created_at DESC");
        let limit_value = limit.unwrap_or(1000);
        query.push_str(&format!(" LIMIT {}", limit_value));

        let rows = sqlx::query(
            &query
        )
        .fetch_all(&self.pool)
        .await?;

        // PgRow -> MarkerClusterInfo 변환
        let mut marker_infos = Vec::new();
        for row in rows {
            marker_infos.push(MarkerClusterInfo {
                id: row.try_get("id").unwrap_or(0),
                member_id: row.try_get("member_id").unwrap_or(0),
                latitude: row.try_get("latitude").unwrap_or(0.0),
                longitude: row.try_get("longitude").unwrap_or(0.0),
                emotion_tag: row.try_get("emotion_tag").unwrap_or_default(),
                description: row.try_get("description").unwrap_or_default(),
                likes: row.try_get("likes").unwrap_or(0),
                dislikes: row.try_get("dislikes").unwrap_or(0),
                views: row.try_get("views").unwrap_or(0),
                author: row.try_get("author").unwrap_or_default(),
                thumbnail_img: row.try_get("thumbnail_img").unwrap_or_default(),
                created_at: row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: row.try_get("updated_at").unwrap_or_else(|_| chrono::Utc::now()),
            });
        }

        // 줌 레벨에 따른 클러스터링 조정
        // 줌 레벨 15 이상에서는 클러스터링을 최소화해서 개별 마커 사진이 많이 보이도록
        let precision = if lat_delta > 2.0 || lng_delta > 2.0 {
            3  // 매우 큰 클러스터 (줌아웃)
        } else if lat_delta > 0.5 || lng_delta > 0.5 {
            4  // 중간 줌에서 적절한 클러스터링
        } else if lat_delta > 0.1 || lng_delta > 0.1 {
            5  // 줌 레벨 14에서 적절한 클러스터링
        } else if lat_delta > 0.03 || lng_delta > 0.03 {
            8  // 줌 레벨 15 이상에서 매우 세밀한 클러스터링 (개별 마커 많이 보임)
        } else {
            9  // 매우 줌인에서 최대 세밀한 클러스터링 (개별 마커 사진 많이 보임)
        };
        // precision이 9 이상이거나 lat_delta/lng_delta가 아주 작으면 클러스터링 없이 개별 마커로 분리
        if precision >= 9 || (lat_delta < 0.01 && lng_delta < 0.01) {
            let all_marker_ids: Vec<i32> = marker_infos.iter().map(|m| m.id).collect();
            use futures::stream::{FuturesUnordered, StreamExt};
            let image_futures: FuturesUnordered<_> = all_marker_ids.iter()
                .map(|&marker_id| {
                    let db = &self.pool;
                    async move {
                        let rows = sqlx::query(
                            r#"
                            SELECT id, marker_id, image_type, image_url, image_order, is_primary, created_at, updated_at
                            FROM bigpicture.marker_images 
                            WHERE marker_id = $1
                            ORDER BY image_order ASC
                            "#
                        )
                        .bind(marker_id)
                        .fetch_all(db)
                        .await
                        .unwrap_or_default();
                        let images: Vec<MarkerImage> = rows.iter().map(|row| MarkerImage {
                            id: row.try_get("id").unwrap_or(0),
                            marker_id: row.try_get("marker_id").unwrap_or(0),
                            image_type: row.try_get("image_type").unwrap_or_default(),
                            image_url: row.try_get("image_url").unwrap_or_default(),
                            image_order: row.try_get("image_order").unwrap_or(0),
                            is_primary: row.try_get("is_primary").unwrap_or(false),
                            created_at: row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now()),
                            updated_at: row.try_get("updated_at").unwrap_or_else(|_| chrono::Utc::now()),
                        }).collect();
                        (marker_id, images)
                    }
                })
                .collect();
            let marker_images_map: std::collections::HashMap<i32, Vec<MarkerImage>> = 
                image_futures.collect::<Vec<_>>().await.into_iter().collect();
            let mut result = Vec::new();
            for m in marker_infos {
                let empty_vec = Vec::new();
                let images = marker_images_map.get(&m.id).unwrap_or(&empty_vec);
                let images_json: Vec<serde_json::Value> = images.iter().map(|img| serde_json::json!({
                    "id": img.id,
                    "markerId": img.marker_id,
                    "imageType": img.image_type,
                    "imageUrl": img.image_url,
                    "imageOrder": img.image_order,
                    "isPrimary": img.is_primary,
                    "createdAt": img.created_at,
                    "updatedAt": img.updated_at
                })).collect();
                result.push(serde_json::json!({
                    "h3_index": null,
                    "lat": m.latitude,
                    "lng": m.longitude,
                    "count": 1,
                    "marker_ids": [m.id],
                    "markers": [serde_json::json!({
                        "id": m.id,
                        "memberId": m.member_id,
                        "latitude": m.latitude,
                        "longitude": m.longitude,
                        "emotionTag": m.emotion_tag,
                        "description": m.description,
                        "likes": m.likes,
                        "dislikes": m.dislikes,
                        "views": m.views,
                        "author": m.author,
                        "thumbnailImg": m.thumbnail_img,
                        "createdAt": m.created_at.to_rfc3339(),
                        "updatedAt": m.updated_at.to_rfc3339(),
                        "images": images_json
                    })]
                }));
            }
            return Ok(result);
        }
        use std::collections::HashMap;
        let mut clusters: HashMap<u64, Vec<MarkerClusterInfo>> = HashMap::new();
        for marker in marker_infos {
            let h3 = H3Cell::from_point(Point::new(marker.longitude, marker.latitude), precision).unwrap();
            let h3idx = h3.h3index();
            clusters.entry(h3idx).or_default().push(marker);
        }

        // 모든 마커 ID 수집
        let all_marker_ids: Vec<i32> = clusters.values()
            .flat_map(|marker_list| marker_list.iter().map(|m| m.id))
            .collect();

        // 비동기 병렬로 모든 마커의 이미지 조회
        use futures::stream::{FuturesUnordered, StreamExt};
        let image_futures: FuturesUnordered<_> = all_marker_ids.iter()
            .map(|&marker_id| {
                let db = &self.pool;
                async move {
                    let rows = sqlx::query(
                        r#"
                        SELECT id, marker_id, image_type, image_url, image_order, is_primary, created_at, updated_at
                        FROM bigpicture.marker_images 
                        WHERE marker_id = $1
                        ORDER BY image_order ASC
                        "#
                    )
                    .bind(marker_id)
                    .fetch_all(db)
                    .await
                    .unwrap_or_default();

                    let images: Vec<MarkerImage> = rows.iter().map(|row| MarkerImage {
                        id: row.try_get("id").unwrap_or(0),
                        marker_id: row.try_get("marker_id").unwrap_or(0),
                        image_type: row.try_get("image_type").unwrap_or_default(),
                        image_url: row.try_get("image_url").unwrap_or_default(),
                        image_order: row.try_get("image_order").unwrap_or(0),
                        is_primary: row.try_get("is_primary").unwrap_or(false),
                        created_at: row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now()),
                        updated_at: row.try_get("updated_at").unwrap_or_else(|_| chrono::Utc::now()),
                    }).collect();

                    (marker_id, images)
                }
            })
            .collect();

        let marker_images_map: std::collections::HashMap<i32, Vec<MarkerImage>> = 
            image_futures.collect::<Vec<_>>().await.into_iter().collect();

        // 병렬 처리를 위한 클러스터 데이터 준비
        let cluster_data: Vec<_> = clusters.into_iter().collect();
        
        // 병렬로 클러스터 처리
        let result: Vec<serde_json::Value> = tokio::task::spawn_blocking(move || {
            cluster_data.into_par_iter().map(|(h3idx, marker_list)| {
                let count = marker_list.len();
                let (sum_lat, sum_lng) = marker_list.iter().fold((0.0, 0.0), |acc, m| (acc.0 + m.latitude, acc.1 + m.longitude));
                let center_lat = sum_lat / count as f64;
                let center_lng = sum_lng / count as f64;
                let marker_ids: Vec<i32> = marker_list.iter().map(|m| m.id).collect();

                // 병렬로 마커 JSON 변환 (이미지 포함)
                let markers: Vec<serde_json::Value> = marker_list.par_iter().map(|m| {
                    let empty_vec = Vec::new();
                    let images = marker_images_map.get(&m.id).unwrap_or(&empty_vec);
                    let images_json: Vec<serde_json::Value> = images.iter().map(|img| serde_json::json!({
                        "id": img.id,
                        "markerId": img.marker_id,
                        "imageType": img.image_type,
                        "imageUrl": img.image_url,
                        "imageOrder": img.image_order,
                        "isPrimary": img.is_primary,
                        "createdAt": img.created_at,
                        "updatedAt": img.updated_at
                    })).collect();

                    serde_json::json!({
                        "id": m.id,
                        "memberId": m.member_id,
                        "latitude": m.latitude,
                        "longitude": m.longitude,
                        "emotionTag": m.emotion_tag,
                        "description": m.description,
                        "likes": m.likes,
                        "dislikes": m.dislikes,
                        "views": m.views,
                        "author": m.author,
                        "thumbnailImg": m.thumbnail_img,
                        "createdAt": m.created_at.to_rfc3339(),
                        "updatedAt": m.updated_at.to_rfc3339(),
                        "images": images_json
                    })
                }).collect();

                serde_json::json!({
                    "h3_index": format!("{:x}", h3idx),
                    "lat": center_lat,
                    "lng": center_lng,
                    "count": count,
                    "marker_ids": marker_ids,
                    "markers": markers
                })
            }).collect()
        }).await?;
        Ok(result)
    }

    pub async fn get_markers_rank(
        &self,
        _lat: f64,
        _lng: f64,
        _lat_delta: f64,
        _lng_delta: f64,
        emotion_tags: Option<Vec<String>>,
        min_likes: Option<i32>,
        min_views: Option<i32>,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        limit: Option<i32>,
        user_id: Option<i64>,
    ) -> Result<Vec<Marker>> {
        let mut query = String::from(
            "SELECT id, member_id, location, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, created_at, updated_at
             FROM bigpicture.markers WHERE 1=1"
        );
        if let Some(tags) = &emotion_tags {
            if !tags.is_empty() {
                let tags_str = tags.iter().map(|tag| format!("'{}'", tag)).collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND emotion_tag IN ({})", tags_str));
            }
        }
        if let Some(likes) = min_likes {
            query.push_str(&format!(" AND likes >= {}", likes));
        }
        if let Some(views) = min_views {
            query.push_str(&format!(" AND views >= {}", views));
        }
        if let Some(uid) = user_id {
            query.push_str(&format!(" AND member_id = {}", uid));
        }
        let allowed_sort = ["created_at", "likes", "views", "dislikes"];
        let sort_col = sort_by.filter(|s| allowed_sort.contains(&s.to_lowercase().as_str())).unwrap_or("likes");
        let order = sort_order.filter(|o| o.eq_ignore_ascii_case("asc") || o.eq_ignore_ascii_case("desc")).unwrap_or("desc");
        query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
        let limit_value = limit.unwrap_or(20);
        query.push_str(&format!(" LIMIT {}", limit_value));

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await?;

        let mut markers = Vec::new();
        for row in rows {
            markers.push(Marker {
                id: row.try_get("id").unwrap_or(0),
                member_id: row.try_get("member_id").ok(),
                location: row.try_get("location").ok(),
                emotion_tag: row.try_get("emotion_tag").ok(),
                description: row.try_get("description").ok(),
                likes: row.try_get("likes").unwrap_or(0),
                dislikes: row.try_get("dislikes").unwrap_or(0),
                views: row.try_get("views").unwrap_or(0),
                author: row.try_get("author").ok(),
                thumbnail_img: row.try_get("thumbnail_img").ok(),
                created_at: row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: row.try_get("updated_at").unwrap_or_else(|_| chrono::Utc::now()),
            });
        }
        Ok(markers)
    }
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize)]
#[serde_with::serde_as]
pub struct OriginalImage {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub id: uuid::Uuid,
    pub filename: String,
    pub original_filename: String,
    pub file_path: String,
    pub file_size_mb: f64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize)]
#[serde_with::serde_as]
pub struct WebpImage {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub id: uuid::Uuid,
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub original_id: uuid::Uuid,
    pub filename: String,
    pub file_path: String,
    pub file_size_mb: f64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub image_type: String, // thumbnail, map
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// 기존 ImageInfo는 호환성을 위해 유지
#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize)]
#[serde_with::serde_as]
pub struct ImageInfo {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub id: uuid::Uuid,
    pub filename: String,
    pub original_filename: String,
    pub file_path: String,
    pub file_size_mb: f64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format: String,
    pub image_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, Debug, serde::Serialize)]
pub struct Marker {
    pub id: i32,
    pub member_id: Option<i64>, // 마커를 생성한 사용자 ID
    pub location: Option<String>, // PostGIS geography 타입 (WKT 형식)
    pub emotion_tag: Option<String>,
    pub description: Option<String>,
    pub likes: i32,
    pub dislikes: i32,
    pub views: i32,
    pub author: Option<String>,
    pub thumbnail_img: Option<String>, // 기존 썸네일 필드 유지
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
pub struct MarkerImage {
    pub id: i32,
    pub marker_id: i32,
    pub image_type: String, // thumbnail, detail, gallery
    pub image_url: String,
    pub image_order: i32,
    pub is_primary: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Marker {
    /// WKT 문자열에서 위도/경도 추출
    pub fn get_latitude(&self) -> Option<f64> {
        self.location.as_ref().and_then(|wkt| {
            // POINT(lng lat) 형식에서 lat 추출
            if wkt.starts_with("POINT(") && wkt.ends_with(")") {
                let coords = &wkt[6..wkt.len()-1]; // POINT( 제거하고 ) 제거
                let parts: Vec<&str> = coords.split_whitespace().collect();
                if parts.len() == 2 {
                    parts[1].parse::<f64>().ok()
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    pub fn get_longitude(&self) -> Option<f64> {
        self.location.as_ref().and_then(|wkt| {
            // POINT(lng lat) 형식에서 lng 추출
            if wkt.starts_with("POINT(") && wkt.ends_with(")") {
                let coords = &wkt[6..wkt.len()-1]; // POINT( 제거하고 ) 제거
                let parts: Vec<&str> = coords.split_whitespace().collect();
                if parts.len() == 2 {
                    parts[0].parse::<f64>().ok()
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct MemberMarker {
    pub id: i64,
    pub member_id: i64,
    pub marker_id: i64,
    pub interaction_type: String, // created, liked, disliked, viewed, bookmarked
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
} 

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct Member {
    pub id: i64,
    pub email: String,
    pub nickname: String,
    pub profile_image_url: Option<String>,
    pub region: Option<String>,
    pub gender: Option<String>,
    pub age: Option<i32>,
    pub personality_type: Option<String>,
    pub is_active: bool,
    pub email_verified: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct AuthProvider {
    pub id: i64,
    pub member_id: i64,
    pub provider_type: String,
    pub provider_id: String,
    pub provider_email: Option<String>,
    pub password_hash: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct Hobby {
    pub id: i32,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct Interest {
    pub id: i32,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct MemberHobby {
    pub id: i32,
    pub member_id: i32,
    pub hobby_id: i32,
    pub proficiency_level: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct MemberInterest {
    pub id: i32,
    pub member_id: i32,
    pub interest_id: i32,
    pub interest_level: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
} 