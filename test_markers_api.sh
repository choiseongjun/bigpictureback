#!/bin/bash

# 마커 조회 API 테스트 스크립트
# 서버가 http://localhost:8080에서 실행 중이라고 가정

BASE_URL="http://localhost:8080"

echo "=== 마커 조회 API 테스트 ==="
echo

# 1. 기본 조회 (서울 시청 근처)
echo "1. 기본 조회 (서울 시청 근처):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 2. 감성 태그 필터링
echo "2. 감성 태그 필터링 (😍,☕):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&emotionTags=😍,☕" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 3. 최소 좋아요 수 필터링
echo "3. 최소 좋아요 수 필터링 (10개 이상):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&minLikes=10" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 4. 최소 조회수 필터링
echo "4. 최소 조회수 필터링 (100회 이상):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&minViews=100" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 5. 좋아요 수로 정렬 (내림차순)
echo "5. 좋아요 수로 정렬 (내림차순):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&sortBy=likes&sortOrder=desc" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 6. 조회수로 정렬 (오름차순)
echo "6. 조회수로 정렬 (오름차순):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&sortBy=views&sortOrder=asc" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 7. 복합 필터링 (감성 태그 + 최소 좋아요 + 정렬)
echo "7. 복합 필터링 (감성 태그 + 최소 좋아요 + 정렬):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&emotionTags=😍,☕&minLikes=10&minViews=100&sortBy=likes&sortOrder=desc" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 8. 다른 지역 조회 (강남역)
echo "8. 다른 지역 조회 (강남역):"
curl -X GET "${BASE_URL}/api/markers?lat=37.4981&lng=127.0276&latDelta=0.05&lngDelta=0.05" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 9. 더 넓은 범위 조회
echo "9. 더 넓은 범위 조회:"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.5&lngDelta=0.5" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "---"
echo

# 10. 생성일 기준 정렬
echo "10. 생성일 기준 정렬 (최신순):"
curl -X GET "${BASE_URL}/api/markers?lat=37.5665&lng=126.9780&latDelta=0.1&lngDelta=0.1&sortBy=created_at&sortOrder=desc" \
  -H "Content-Type: application/json" \
  | jq '.'
echo
echo "=== 테스트 완료 ===" 