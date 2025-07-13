use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use image::imageops::{resize, FilterType};
use imageproc::drawing::draw_filled_circle;
use std::path::Path;
use anyhow::Result;
use webp::{Encoder, WebPMemory};
use log::info;

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
        // 파일 크기 확인
        let file_size_mb = self.get_file_size_mb(image_data);
        info!("🖼️ 이미지 처리 시작: {:.2}MB", file_size_mb);
        

        
        // 이미지 디코딩
        let img = image::load_from_memory(image_data)?;
        let (width, height) = img.dimensions();
        
        // 이미 적절한 크기인 경우 WebP 변환만 수행
        if width <= self.max_width && height <= self.max_height {
            info!("📏 이미지 크기 적절함 - WebP 변환만 수행");
            let rgba = img.to_rgba8();
            let encoder = Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
            let webp_data: WebPMemory = encoder.encode(80.0);
            return Ok(webp_data.to_vec());
        }
        
        // 리사이즈가 필요한 경우
        info!("📐 이미지 리사이즈 필요: {}x{} -> {}x{}", width, height, self.max_width, self.max_height);
        let resized = self.resize_image(img);
        
        // WebP로 인코딩
        let rgba = resized.to_rgba8();
        let encoder = Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
        let webp_data: WebPMemory = encoder.encode(80.0);
        
        let processed_size_mb = webp_data.len() as f64 / (1024.0 * 1024.0);
        info!("✅ 이미지 처리 완료: {:.2}MB -> {:.2}MB", file_size_mb, processed_size_mb);
        
        Ok(webp_data.to_vec())
    }

    pub fn process_circular_thumbnail(&self, image_data: &[u8]) -> Result<Vec<u8>> {
        let file_size_mb = self.get_file_size_mb(image_data);
        info!("🔄 원형 썸네일 처리 시작: {:.2}MB", file_size_mb);
        
        // 이미지 디코딩
        let img = image::load_from_memory(image_data)?;
        let (width, height) = img.dimensions();
        
        // 원형 썸네일은 최대 500x500으로 제한 (S3 업로드 안정성)
        let max_circular_size = 500u32;
        let processed_img = if width > max_circular_size || height > max_circular_size || file_size_mb > 5.0 {
            info!("📐 원형 썸네일 크기 제한 - 리사이즈: {}x{} -> {}x{}", width, height, max_circular_size, max_circular_size);
            
            // 비율을 유지하면서 최대 크기로 리사이즈
            let ratio = (max_circular_size as f32 / width as f32).min(max_circular_size as f32 / height as f32);
            let new_width = (width as f32 * ratio) as u32;
            let new_height = (height as f32 * ratio) as u32;
            
            img.resize(new_width, new_height, image::imageops::FilterType::Nearest)
        } else {
            img
        };
        
        // 정사각형으로 크롭
        let cropped = self.crop_to_square(processed_img);
        
        // 원형으로 마스킹하고 흰색 테두리 추가
        let circular = self.make_circular_with_border(cropped);
        
        // WebP로 인코딩
        let rgba = circular.to_rgba8();
        let encoder = Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
        let webp_data: WebPMemory = encoder.encode(self.quality as f32);
        
        let processed_size_mb = webp_data.len() as f64 / (1024.0 * 1024.0);
        info!("✅ 원형 썸네일 처리 완료: {:.2}MB -> {:.2}MB", file_size_mb, processed_size_mb);
        
        Ok(webp_data.to_vec())
    }

    fn resize_image(&self, img: DynamicImage) -> DynamicImage {
        let (width, height) = img.dimensions();
        
        // 이미지가 최대 크기보다 작으면 리사이즈하지 않음
        if width <= self.max_width && height <= self.max_height {
            return img;
        }
        
        // 비율을 유지하면서 리사이즈
        let ratio = (self.max_width as f32 / width as f32).min(self.max_height as f32 / height as f32);
        let new_width = (width as f32 * ratio) as u32;
        let new_height = (height as f32 * ratio) as u32;
        
        info!("📐 리사이즈: {}x{} -> {}x{} (비율: {:.2})", width, height, new_width, new_height, ratio);
        
        // 큰 이미지의 경우 더 빠른 필터 사용
        let filter = if width > 2000 || height > 2000 {
            image::imageops::FilterType::Nearest // 가장 빠른 필터
        } else if width > 1000 || height > 1000 {
            image::imageops::FilterType::Triangle // 중간 속도
        } else {
            image::imageops::FilterType::Lanczos3 // 고품질
        };
        
        img.resize(new_width, new_height, filter)
    }

    fn crop_to_square(&self, img: DynamicImage) -> DynamicImage {
        let (width, height) = img.dimensions();
        let size = width.min(height);
        
        // 중앙에서 정사각형 크롭
        let x = (width - size) / 2;
        let y = (height - size) / 2;
        
        img.crop_imm(x, y, size, size)
    }

    fn make_circular_with_border(&self, img: DynamicImage) -> DynamicImage {
        let size = img.width().min(img.height());
        let border_width = 4u32; // 흰색 테두리 두께
        let shadow_offset = 3u32; // 그림자 오프셋
        let total_size = size + (border_width * 2) + shadow_offset;
        
        // 투명 배경의 새 이미지 생성
        let mut output = RgbaImage::new(total_size, total_size);
        
        // 투명으로 채우기
        for pixel in output.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
        
        // 그림자 그리기 (약간 아래쪽으로 오프셋)
        let shadow_center = (total_size / 2) + shadow_offset;
        let shadow_radius = (size / 2) + border_width;
        
        for y in 0..total_size {
            for x in 0..total_size {
                let dx = if x > shadow_center { x - shadow_center } else { shadow_center - x };
                let dy = if y > shadow_center { y - shadow_center } else { shadow_center - y };
                let distance_squared = dx * dx + dy * dy;
                
                if distance_squared <= shadow_radius * shadow_radius {
                    // 그림자 영역 (반투명 검은색)
                    output.put_pixel(x, y, Rgba([0, 0, 0, 80]));
                }
            }
        }
        
        // 원형 이미지와 테두리 그리기
        let center = total_size / 2;
        let radius = size / 2;
        
        for y in 0..total_size {
            for x in 0..total_size {
                let dx = if x > center { x - center } else { center - x };
                let dy = if y > center { y - center } else { center - y };
                let distance_squared = dx * dx + dy * dy;
                
                if distance_squared <= (radius + border_width) * (radius + border_width) {
                    if distance_squared <= radius * radius {
                        // 원형 이미지 영역
                        let src_x = (x as i32 - center as i32) + (size / 2) as i32;
                        let src_y = (y as i32 - center as i32) + (size / 2) as i32;
                        
                        if src_x >= 0 && src_x < size as i32 && src_y >= 0 && src_y < size as i32 {
                            let pixel = img.get_pixel(src_x as u32, src_y as u32);
                            output.put_pixel(x, y, pixel);
                        }
                    } else {
                        // 흰색 테두리 영역
                        output.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                    }
                }
            }
        }
        
        DynamicImage::ImageRgba8(output)
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