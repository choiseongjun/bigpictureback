use rusoto_core::{Region, HttpClient};
use rusoto_credential::{StaticProvider, ProvideAwsCredentials};
use rusoto_s3::{S3Client, S3, PutObjectRequest};
use anyhow::Result;
use log::{info, error};
use std::path::Path;
use uuid::Uuid;
use chrono::Utc;

#[derive(Clone)]
pub struct S3Service {
    client: S3Client,
    bucket_name: String,
    region: String,
}

impl S3Service {
    pub async fn new(bucket_name: String, region: String, access_key: String, secret_key: String) -> Result<Self> {
        let credentials = StaticProvider::new_minimal(access_key, secret_key);
        let region_name = region.clone();
        
        // 리전별 엔드포인트 설정
        let region = match region_name.as_str() {
            "us-east-1" => Region::UsEast1,
            "us-west-1" => Region::UsWest1,
            "us-west-2" => Region::UsWest2,
            "eu-west-1" => Region::EuWest1,
            "eu-central-1" => Region::EuCentral1,
            "ap-southeast-1" => Region::ApSoutheast1,
            "ap-southeast-2" => Region::ApSoutheast2,
            "ap-northeast-1" => Region::ApNortheast1,
            "ap-northeast-2" => Region::ApNortheast2,
            "sa-east-1" => Region::SaEast1,
            _ => Region::Custom {
                name: region_name.clone(),
                endpoint: format!("https://s3.{}.amazonaws.com", region_name),
            }
        };
        
        // HTTP 클라이언트 설정 개선
        let http_client = HttpClient::new()?;
        
        let client = S3Client::new_with(http_client, credentials, region);
        
        info!("✅ S3 클라이언트 초기화 완료 - 버킷: {}, 리전: {}", bucket_name, region_name);
        
        Ok(Self {
            client,
            bucket_name,
            region: region_name,
        })
    }

    pub async fn upload_file(&self, data: Vec<u8>, key: &str, content_type: &str) -> Result<String> {
        info!("📤 S3 업로드 시작: {}", key);
        info!("📤 버킷: {}, 리전: {}", self.bucket_name, self.region);
        
        let put_request = PutObjectRequest {
            bucket: self.bucket_name.clone(),
            key: key.to_string(),
            body: Some(data.into()),
            content_type: Some(content_type.to_string()),
            ..Default::default()
        };
        
        match self.client.put_object(put_request).await {
            Ok(result) => {
                let url = format!("https://{}.s3.{}.amazonaws.com/{}", self.bucket_name, self.region, key);
                info!("✅ S3 업로드 완료: {}", url);
                info!("✅ ETag: {:?}", result.e_tag);
                Ok(url)
            }
            Err(e) => {
                error!("❌ S3 업로드 실패: {:?}", e);
                Err(anyhow::anyhow!("S3 업로드 실패: {:?}", e))
            }
        }
    }

    pub async fn upload_thumbnail(&self, image_data: Vec<u8>, _original_filename: &str) -> Result<String> {
        let timestamp = Utc::now().timestamp();
        let uuid = Uuid::new_v4().to_string()[..8].to_string();
        // 무조건 webp로 저장
        let key = format!("thumbnails/{}_{}_{}.webp", "thumbnail", uuid, timestamp);
        let content_type = "image/webp";
        self.upload_file(image_data, &key, content_type).await
    }

    pub async fn upload_circular_thumbnail(&self, image_data: Vec<u8>, _original_filename: &str) -> Result<String> {
        let timestamp = Utc::now().timestamp();
        let uuid = Uuid::new_v4().to_string()[..8].to_string();
        let key = format!("thumbnails/circular_{}_{}_{}.webp", "thumbnail", uuid, timestamp);
        self.upload_file(image_data, &key, "image/webp").await
    }

    pub async fn upload_map_image(&self, image_data: Vec<u8>, _original_filename: &str) -> Result<String> {
        let timestamp = Utc::now().timestamp();
        let uuid = Uuid::new_v4().to_string()[..8].to_string();
        let key = format!("maps/{}_{}_{}.webp", "map", uuid, timestamp);
        let content_type = "image/webp";
        self.upload_file(image_data, &key, content_type).await
    }

    pub async fn delete_file(&self, key: &str) -> Result<()> {
        info!("🗑️ S3 파일 삭제: {}", key);
        
        let delete_request = rusoto_s3::DeleteObjectRequest {
            bucket: self.bucket_name.clone(),
            key: key.to_string(),
            ..Default::default()
        };
        
        self.client.delete_object(delete_request).await?;
        
        info!("✅ S3 파일 삭제 완료: {}", key);
        
        Ok(())
    }

    pub fn get_file_url(&self, key: &str) -> String {
        format!("https://{}.s3.{}.amazonaws.com/{}", self.bucket_name, self.region, key)
    }
} 