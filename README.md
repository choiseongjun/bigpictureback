# BigPicture Backend (Rust + Actix-web)

Rust Actix-web 프레임워크를 사용한 백엔드 API 서버입니다. 모든 이미지를 WebP로 변환하여 용량을 최적화합니다.

## 🚀 시작하기

### 필수 요구사항

- Rust 1.70 이상
- Cargo

### 설치 및 실행

1. 의존성 설치 및 빌드:
```bash
cargo build
```

2. 서버 실행:
```bash
cargo run
```

서버는 `http://localhost:5500`에서 실행됩니다.

## 📁 프로젝트 구조

```
bigpictureback/
├── src/
│   ├── main.rs           # 메인 애플리케이션 진입점
│   ├── image_processor.rs # 이미지 처리 및 WebP 변환
│   └── routes.rs         # API 라우트 핸들러들
├── uploads/              # 업로드된 이미지 저장소
│   ├── thumbnail/        # 썸네일 이미지
│   └── map/             # 지도용 이미지
├── Cargo.toml           # Rust 프로젝트 설정
└── README.md            # 프로젝트 문서
```

## 🔗 API 엔드포인트

### 기본 엔드포인트
- `GET /` - API 상태 확인
- `GET /api/health` - 헬스체크

### 이미지 관련 엔드포인트
- `POST /api/images/upload/thumbnail` - 썸네일 이미지 업로드 (300x300, WebP 변환)
- `POST /api/images/upload/map` - 지도용 이미지 업로드 (800x600, WebP 변환)
- `GET /api/images/info/{filename}` - 이미지 정보 조회
- `GET /api/images/download/{filename}` - WebP 이미지 다운로드

## 🛠️ 사용된 기술

- **Framework**: [Actix-web](https://actix.rs/) - Rust 웹 프레임워크
- **Image Processing**: 
  - [image](https://crates.io/crates/image) - 이미지 처리 라이브러리
  - WebP 변환 - 모든 이미지를 WebP로 자동 변환
- **Async/Await**: 비동기 처리
- **Error Handling**: anyhow를 사용한 에러 처리

## 📝 예제 요청

### 서버 상태 확인
```bash
curl http://localhost:5500/
```

### 헬스체크
```bash
curl http://localhost:5500/api/health
```

### 썸네일 이미지 업로드 (WebP 변환)
```bash
curl -X POST -F "image=@thumbnail.jpg" http://localhost:5500/api/images/upload/thumbnail
```

### 지도용 이미지 업로드 (WebP 변환)
```bash
curl -X POST -F "image=@map_image.jpg" http://localhost:5500/api/images/upload/map
```

### 이미지 정보 조회
```bash
curl http://localhost:5500/api/images/info/filename.webp
```

### WebP 이미지 다운로드
```bash
curl http://localhost:5500/api/images/download/filename.webp
```

## 🎯 주요 기능

### WebP 변환
- **모든 업로드 이미지가 자동으로 WebP로 변환**됩니다
- 원본: jpg, png, gif, bmp 등 → 변환: WebP
- 용량 최적화로 트래픽 절약

### 이미지 최적화
- **썸네일**: 300x300px, 80% 품질
- **지도용**: 800x600px, 85% 품질
- 비율 유지하면서 자동 리사이징

### 파일 관리
- 고유한 파일명 생성 (UUID + 타임스탬프)
- 자동 디렉토리 생성
- 파일 형식 검증
- 10MB 파일 크기 제한

## 🔧 개발

### 새로운 라우트 추가

`src/routes.rs` 파일에서 새로운 엔드포인트를 추가할 수 있습니다:

```rust
pub fn setup_routes(config: &mut web::ServiceConfig) {
    config
        .service(
            web::scope("/api")
                .route("/new-endpoint", web::get().to(new_handler))
        );
}

async fn new_handler() -> Result<HttpResponse> {
    // 핸들러 로직
}
```

### 이미지 처리 설정 변경

`src/image_processor.rs`에서 이미지 처리 옵션을 수정할 수 있습니다:

```rust
pub fn create_thumbnail_processor() -> ImageProcessor {
    ImageProcessor::new(400, 400, 90) // 크기와 품질 조정
}
```

## 📊 성능 최적화

### WebP 변환 효과
- **JPEG 대비 25-35% 용량 감소**
- **PNG 대비 50-80% 용량 감소**
- 빠른 로딩 속도
- 모던 브라우저 지원

### 트래픽 절약
- 지도에 많은 썸네일 이미지 표시 시 트래픽 대폭 감소
- 모바일 환경에서 데이터 사용량 절약

## 📄 라이선스

이 프로젝트는 MIT 라이선스 하에 배포됩니다. 