use image::{DynamicImage, GenericImageView};
use std::path::Path;
use anyhow::Result;
use webp::{Encoder, WebPMemory};

pub struct ImageProcessor {
    pub max_width: u32,
    pub max_height: u32,
    pub quality: u8,
}

impl ImageProcessor {
    pub fn new(max_width: u32, max_height: u32, quality: u8) -> Self {
        Self {
            max_width,
            max_height,
            quality,
        }
    }

    pub fn process_image(&self, image_data: &[u8]) -> Result<Vec<u8>> {
        // 이미지 디코딩
        let img = image::load_from_memory(image_data)?;
        
        // 이미지 리사이즈
        let resized = self.resize_image(img);
        
        // WebP로 인코딩
        let rgba = resized.to_rgba8();
        let encoder = Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
        let webp_data: WebPMemory = encoder.encode(self.quality as f32);
        
        Ok(webp_data.to_vec())
    }

    fn resize_image(&self, img: DynamicImage) -> DynamicImage {
        let (width, height) = img.dimensions();
        
        // 이미지가 최대 크기보다 작으면 리사이즈하지 않음
        if width <= self.max_width && height <= self.max_height {
            return img;
        }
        
        // 비율을 유지하면서 리사이즈
        img.resize(self.max_width, self.max_height, image::imageops::FilterType::Lanczos3)
    }

    pub fn get_image_info(&self, image_data: &[u8]) -> Result<(u32, u32, String)> {
        let img = image::load_from_memory(image_data)?;
        let (width, height) = img.as_rgba8().map_or((0, 0), |rgba| rgba.dimensions());
        
        // 이미지 형식 감지 (간단한 방법)
        let format = if image_data.len() > 2 {
            match &image_data[0..2] {
                [0xFF, 0xD8] => "JPEG",
                [0x89, 0x50] => "PNG",
                [0x47, 0x49] => "GIF",
                [0x42, 0x4D] => "BMP",
                _ => "Unknown",
            }
        } else {
            "Unknown"
        };
        
        Ok((width, height, format.to_string()))
    }

    pub fn is_valid_image_format(&self, filename: &str) -> bool {
        let ext = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
            
        matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp")
    }

    pub fn get_file_size_mb(&self, data: &[u8]) -> f64 {
        data.len() as f64 / (1024.0 * 1024.0)
    }
}

// 편의 함수들
pub fn create_thumbnail_processor() -> ImageProcessor {
    ImageProcessor::new(300, 300, 80)
}

pub fn create_map_processor() -> ImageProcessor {
    ImageProcessor::new(800, 600, 85)
} 