#!/bin/bash

echo "🚀 BigPicture Backend 배포 시작..."

# 1. 릴리즈 빌드
echo "📦 릴리즈 빌드 중..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ 빌드 실패"
    exit 1
fi

echo "✅ 빌드 완료"

# 2. 서비스 중지 (이미 실행 중인 경우)
echo "🛑 기존 서비스 중지 중..."
sudo systemctl stop bigpictureback 2>/dev/null || true

# 3. 바이너리 복사
echo "📋 바이너리 복사 중..."
sudo cp target/release/bigpictureback /usr/local/bin/
sudo chmod +x /usr/local/bin/bigpictureback

# 4. 서비스 파일 복사
echo "📄 서비스 파일 설정 중..."
sudo cp bigpictureback.service /etc/systemd/system/

# 5. systemd 재로드
echo "🔄 systemd 재로드 중..."
sudo systemctl daemon-reload

# 6. 서비스 시작
echo "▶️ 서비스 시작 중..."
sudo systemctl enable bigpictureback
sudo systemctl start bigpictureback

# 7. 상태 확인
echo "📊 서비스 상태 확인 중..."
sudo systemctl status bigpictureback

echo "🎉 배포 완료!"
echo "📍 서비스 주소: http://localhost:5500" 