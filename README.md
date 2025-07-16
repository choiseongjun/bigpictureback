# BigPicture Backend (Rust + Actix-web)

Rust Actix-web 프레임워크를 사용한 백엔드 API 서버입니다. 모든 이미지를 WebP로 변환하여 용량을 최적화하고, 구글, 카카오, 네이버, 이메일 등 다양한 소셜 로그인을 지원합니다.

## 🚀 시작하기

### 필수 요구사항

- Rust 1.70 이상
- Cargo
- PostgreSQL

### 설치 및 실행

1. 환경변수 설정:
```bash
# env.example을 .env로 복사
cp env.example .env

# .env 파일을 편집하여 설정을 변경
# DATABASE_URL=postgresql://postgres:123@localhost:5432/bigpicture
# SERVER_PORT=5500
# 등등...
```

2. 의존성 설치 및 빌드:
```bash
cargo build
```

3. 서버 실행:
```bash
cargo run
```

서버는 `http://localhost:5500`에서 실행됩니다.

## 📁 프로젝트 구조

```
bigpictureback/
├── src/
│   ├── main.rs           # 메인 애플리케이션 진입점
│   ├── config.rs         # 설정 관리
│   ├── database.rs       # 데이터베이스 연결 및 쿼리
│   ├── image_processor.rs # 이미지 처리 및 WebP 변환
│   ├── routes.rs         # API 라우트 핸들러들
│   ├── s3_service.rs     # S3 서비스
│   └── s3_routes.rs      # S3 관련 라우트
├── uploads/              # 업로드된 이미지 저장소
│   ├── thumbnail/        # 썸네일 이미지
│   └── map/             # 지도용 이미지
├── Cargo.toml           # Rust 프로젝트 설정
├── env.example          # 환경변수 예시 파일
├── social_auth_curl_commands.txt # 소셜 로그인 API 테스트 명령어
└── README.md            # 프로젝트 문서
```

## 🔗 API 엔드포인트

### 기본 엔드포인트
- `GET /` - API 상태 확인
- `GET /api/health` - 헬스체크

### 인증 관련 엔드포인트
- `POST /api/auth/register` - 소셜 로그인 회원가입 (구글, 카카오, 네이버, 이메일)
- `POST /api/auth/login` - 이메일/비밀번호 로그인
- `POST /api/auth/social-login` - 소셜 로그인 (기존 계정 확인)
- `GET /api/members` - 회원 목록 조회
- `GET /api/members/{id}` - 특정 회원 조회

### 이미지 관련 엔드포인트
- `POST /api/images/upload/thumbnail` - 썸네일 이미지 업로드 (300x300, WebP 변환)
- `POST /api/images/upload/map` - 지도용 이미지 업로드 (800x600, WebP 변환)
- `POST /api/images/generate/thumbnail` - 원형 썸네일 생성 (250x250, WebP 변환)
- `GET /api/images/info/{filename}` - 이미지 정보 조회
- `GET /api/images/download/{filename}` - WebP 이미지 다운로드
- `GET /api/images/download/original/{filename}` - 원본 이미지 다운로드
- `GET /api/images/list` - 이미지 목록 조회 (전체)
- `GET /api/images/list?type=thumbnail` - 썸네일 이미지 목록 조회
- `GET /api/images/list?type=map` - 지도용 이미지 목록 조회
- `GET /api/images/stats` - 이미지 통계 조회

### S3 관련 엔드포인트
- `POST /api/s3/upload/thumbnail` - S3 썸네일 업로드
- `POST /api/s3/upload/map` - S3 지도 이미지 업로드
- `POST /api/s3/upload/circular` - S3 원형 썸네일 업로드

### 마커 관련 엔드포인트
- `GET /api/markers` - 지도 마커 조회 (위치, 감성 태그, 정렬 등)

## 🔐 소셜 로그인 지원

### 지원하는 로그인 방식
- **Google** - 구글 OAuth
- **Kakao** - 카카오 로그인
- **Naver** - 네이버 로그인
- **Meta** - 페이스북/인스타그램 로그인
- **Email** - 이메일/비밀번호 로그인

### 주요 기능
- **통합 회원가입**: 하나의 API로 모든 소셜 로그인 처리
- **계정 연결**: 같은 이메일로 여러 소셜 로그인 연결 가능
- **자동 로그인**: 기존 계정 발견 시 자동 로그인 처리
- **이메일 인증**: 소셜 로그인은 자동 이메일 인증 완료

### 데이터베이스 구조
- `members` 테이블: 회원 기본 정보
- `auth_providers` 테이블: 소셜 로그인 제공자 정보
- `member_hobbies` 테이블: 회원 취미 정보
- `member_interests` 테이블: 회원 관심사 정보

## 🛠️ 사용된 기술

- **Framework**: [Actix-web](https://actix.rs/) - Rust 웹 프레임워크
- **Database**: PostgreSQL - 관계형 데이터베이스
- **ORM**: SQLx - 비동기 SQL 쿼리 빌더
- **Image Processing**: 
  - [image](https://crates.io/crates/image) - 이미지 처리 라이브러리
  - WebP 변환 - 모든 이미지를 WebP로 자동 변환
- **Async/Await**: 비동기 처리
- **Error Handling**: anyhow를 사용한 에러 처리
- **Configuration**: dotenv (환경변수 관리)
- **Cloud Storage**: AWS S3 지원

## 📝 예제 요청

### 서버 상태 확인
```bash
curl http://localhost:5500/
```

### 헬스체크
```bash
curl http://localhost:5500/api/health
```

### 구글 소셜 로그인 회원가입
```bash
curl -X POST http://localhost:5500/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@gmail.com",
    "nickname": "구글유저",
    "provider_type": "google",
    "provider_id": "google_123456789",
    "provider_email": "user@gmail.com",
    "profile_image_url": "https://lh3.googleusercontent.com/a/example",
    "region": "서울",
    "gender": "male",
    "age": 25,
    "personality_type": "ENFP"
  }'
```

### 카카오 소셜 로그인 회원가입
```bash
curl -X POST http://localhost:5500/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@kakao.com",
    "nickname": "카카오유저",
    "provider_type": "kakao",
    "provider_id": "kakao_987654321",
    "provider_email": "user@kakao.com",
    "profile_image_url": "http://k.kakaocdn.net/example.jpg",
    "region": "부산",
    "gender": "female",
    "age": 30,
    "personality_type": "ISTJ"
  }'
```

### 이메일/비밀번호 회원가입
```bash
curl -X POST http://localhost:5500/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "nickname": "이메일유저",
    "provider_type": "email",
    "provider_id": "user@example.com",
    "provider_email": "user@example.com",
    "password": "mypassword123",
    "profile_image_url": "https://example.com/profile.jpg",
    "region": "대구",
    "gender": "other",
    "age": 28,
    "personality_type": "INTJ"
  }'
```

### 이메일/비밀번호 로그인
```bash
curl -X POST http://localhost:5500/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "mypassword123"
  }'
```

### 소셜 로그인 (기존 계정 확인)
```bash
curl -X POST http://localhost:5500/api/auth/social-login \
  -H "Content-Type: application/json" \
  -d '{
    "provider_type": "google",
    "provider_id": "google_123456789",
    "provider_email": "user@gmail.com",
    "nickname": "구글유저",
    "profile_image_url": "https://lh3.googleusercontent.com/a/example"
  }'
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

### 이미지 목록 조회
```bash
curl http://localhost:5500/api/images/list
```

### 썸네일 이미지 목록 조회
```bash
curl http://localhost:5500/api/images/list?type=thumbnail
```

### 이미지 통계 조회
```bash
curl http://localhost:5500/api/images/stats
```

### 회원 목록 조회
```bash
curl "http://localhost:5500/api/members?limit=10"
```

### 특정 회원 조회
```bash
curl http://localhost:5500/api/members/1
```

## 🎯 주요 기능

### 소셜 로그인 시스템
- **통합 인증**: 하나의 API로 모든 소셜 로그인 처리
- **계정 연결**: 같은 이메일로 여러 소셜 로그인 연결
- **자동 로그인**: 기존 계정 발견 시 자동 처리
- **보안**: 트랜잭션 기반 안전한 회원 생성

### WebP 변환
- **모든 업로드 이미지가 자동으로 WebP로 변환**됩니다
- 원본: jpg, png, gif, bmp 등 → 변환: WebP
- 용량 최적화로 트래픽 절약

### 이미지 최적화
- **썸네일**: 300x300px, 80% 품질
- **지도용**: 800x600px, 85% 품질
- **원형 썸네일**: 250x250px, 85% 품질, 원형 마스킹
- 비율 유지하면서 자동 리사이징

### 파일 관리
- 고유한 파일명 생성 (UUID + 타임스탬프)
- 자동 디렉토리 생성
- 파일 형식 검증
- 30MB 파일 크기 제한
- 원본 파일 보존

### 데이터베이스 최적화
- 인덱스 기반 빠른 조회
- 트랜잭션 기반 안전한 데이터 처리
- 스키마 기반 구조화된 데이터 관리

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

### 데이터베이스 쿼리 추가

`src/database.rs`에서 새로운 데이터베이스 함수를 추가할 수 있습니다:

```rust
impl Database {
    pub async fn new_function(&self, param: &str) -> Result<Vec<Member>> {
        let members = sqlx::query_as::<_, Member>(
            "SELECT * FROM bigpicture.members WHERE condition = $1"
        )
        .bind(param)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(members)
    }
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

### 데이터베이스 최적화
- 인덱스 기반 빠른 조회
- 트랜잭션 기반 안전한 데이터 처리
- 비동기 쿼리로 높은 동시성 지원

## 📄 라이선스

이 프로젝트는 MIT 라이선스 하에 배포됩니다. 