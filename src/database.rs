use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
use anyhow::Result;
use crate::config::Config;
use log::{info, warn, error};

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
        
        // markers 테이블 생성
        println!("📋 markers 테이블 생성 중...");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bigpicture.markers (
                id BIGSERIAL PRIMARY KEY,
                latitude DOUBLE PRECISION NOT NULL,
                longitude DOUBLE PRECISION NOT NULL,
                emotion_tag VARCHAR(10) NOT NULL,
                description TEXT NOT NULL,
                likes INTEGER DEFAULT 0,
                dislikes INTEGER DEFAULT 0,
                views INTEGER DEFAULT 0,
                author VARCHAR(50) DEFAULT '익명',
                thumbnail_img VARCHAR(500),
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                
                CONSTRAINT check_latitude CHECK (latitude BETWEEN -90 AND 90),
                CONSTRAINT check_longitude CHECK (longitude BETWEEN -180 AND 180),
                CONSTRAINT check_likes CHECK (likes >= 0),
                CONSTRAINT check_dislikes CHECK (dislikes >= 0),
                CONSTRAINT check_views CHECK (views >= 0)
            )
            "#
        )
        .execute(pool)
        .await?;
        println!("✅ markers 테이블 생성 완료");
        
        // markers 인덱스 - 성능 최적화
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_location ON bigpicture.markers(latitude, longitude)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_emotion_tag ON bigpicture.markers(emotion_tag)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_likes ON bigpicture.markers(likes)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_views ON bigpicture.markers(views)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_created_at ON bigpicture.markers(created_at)")
            .execute(pool)
            .await?;
        
        // 복합 인덱스 추가 - 자주 사용되는 조합
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_location_emotion ON bigpicture.markers(latitude, longitude, emotion_tag)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_location_likes ON bigpicture.markers(latitude, longitude, likes)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_markers_location_views ON bigpicture.markers(latitude, longitude, views)")
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
        
        if original_exists && webp_exists && markers_exists {
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
    ) -> Result<Vec<Marker>> {
        info!("🗄️ 데이터베이스 쿼리 시작:");
        
        let lat_min = lat - lat_delta / 2.0;
        let lat_max = lat + lat_delta / 2.0;
        let lng_min = lng - lng_delta / 2.0;
        let lng_max = lng + lng_delta / 2.0;
        
        info!("   - 검색 범위: lat({} ~ {}), lng({} ~ {})", lat_min, lat_max, lng_min, lng_max);
        
        // 정렬
        let sort_column = match sort_by {
            Some("likes") => "likes",
            Some("views") => "views",
            Some("created_at") => "created_at",
            _ => "created_at", // 기본값
        };
        
        let sort_direction = match sort_order {
            Some("asc") => "ASC",
            Some("desc") => "DESC",
            _ => "DESC", // 기본값
        };
        
        info!("   - 정렬: {} {}", sort_column, sort_direction);
        
        let mut query = format!(
            "SELECT id, latitude, longitude, emotion_tag, description, likes, dislikes, views, author, thumbnail_img, created_at, updated_at 
             FROM bigpicture.markers 
             WHERE latitude BETWEEN {} AND {} 
             AND longitude BETWEEN {} AND {}",
            lat_min, lat_max, lng_min, lng_max
        );
        
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
        
        query.push_str(&format!(" ORDER BY {} {}", sort_column, sort_direction));
        
        // LIMIT 추가 (기본값 100개)
        let limit_value = limit.unwrap_or(100);
        query.push_str(&format!(" LIMIT {}", limit_value));
        
        info!("   - 최종 SQL 쿼리: {}", query);
        
        // 쿼리 실행
        let markers = sqlx::query_as::<_, Marker>(&query)
            .fetch_all(&self.pool)
            .await?;
        
        info!("   - 쿼리 실행 완료: {}개 결과", markers.len());
        
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
    pub id: i64,
    pub latitude: f64,
    pub longitude: f64,
    pub emotion_tag: String,
    pub description: String,
    pub likes: i32,
    pub dislikes: i32,
    pub views: i32,
    pub author: String,
    pub thumbnail_img: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
} 