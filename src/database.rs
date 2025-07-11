use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
use anyhow::Result;
use crate::config::Config;

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
        
        if original_exists && webp_exists {
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