# blog-mirror

네이버 블로그 게시물을 자동으로 GitHub Pages(Zola) 블로그에 복제하는 도구입니다.

---

## 동작 방식

```
네이버 블로그 → (fetch) → PostgreSQL DB → (publish) → GitHub 블로그 저장소
```

1. **fetch**: 네이버 블로그 API를 통해 신규 게시물 목록과 본문 HTML을 수집해 DB에 저장
2. **publish**: DB에 저장된 게시물을 Markdown으로 변환하고 GitHub 저장소에 커밋 & 푸시

---

## 사전 요구사항

- Rust 1.87+
- PostgreSQL
- GitHub Personal Access Token (repo 권한)
- Zola 기반 GitHub Pages 블로그 저장소

---

## 설치

```bash
git clone https://github.com/your-username/blog-mirror.git
cd blog-mirror
cargo build --release
```

빌드된 바이너리는 `target/release/blog-mirror`에 위치합니다.

---

## 환경 설정

프로젝트 루트에 `.env` 파일을 생성합니다.

```env
# PostgreSQL 연결 URL
DATABASE_URL=postgres://user:password@localhost/blog_mirror

# 복제할 네이버 블로그 ID
NAVER_BLOG_ID=your_naver_blog_id

# GitHub 블로그 저장소 로컬 경로 (없으면 자동 clone)
GITHUB_REPO_PATH=/path/to/local/github-blog-clone

# GitHub 저장소 원격 URL
GITHUB_REMOTE_URL=https://github.com/username/blog.git

# GitHub 사용자명
GITHUB_USERNAME=your_github_username

# GitHub Personal Access Token
GITHUB_TOKEN=ghp_your_personal_access_token

# 네이버 크롤링 딜레이 (밀리초, 기본값: 1000)
CRAWL_DELAY_MS=1500
```

---

## 커맨드

### `init` — 초기 전체 동기화

네이버 블로그의 모든 게시물을 DB에 수집합니다. **최초 1회만 실행**합니다.

```bash
blog-mirror init
```

- DB 마이그레이션 자동 실행
- 중단되어도 커서 기반으로 재시작 시 이어서 수집

---

### `fetch` — 신규 게시물 수집 (One-shot)

마지막 커서 이후 새로 올라온 게시물을 DB에 저장하고 종료합니다.

```bash
blog-mirror fetch
```

---

### `publish` — GitHub 블로그에 복제 (One-shot)

DB에서 아직 복제되지 않은 게시물을 Markdown으로 변환해 GitHub 저장소에 커밋 & 푸시하고 종료합니다.

```bash
blog-mirror publish
```

복제 대상은 DB `categories` 테이블에서 `should_mirror = true`로 설정된 카테고리의 게시물입니다.

---

### `sync-loop` — 자동 반복 실행

`fetch` → `publish` 순서로 지정된 주기마다 반복 실행합니다. `Ctrl+C`로 종료합니다.

```bash
# 기본값: 3600초(1시간) 주기
blog-mirror sync-loop

# 커스텀 주기 (초 단위)
blog-mirror sync-loop --interval 1800
```

---

### `sync-categories` — 카테고리 동기화 (One-shot)

네이버 블로그의 카테고리 목록(이름, 계층 구조)을 DB에 동기화하고 종료합니다.

```bash
blog-mirror sync-categories
```

---

## 초기 설정 절차

### 1. DB 준비

```bash
createdb blog_mirror
```

마이그레이션은 각 커맨드 실행 시 자동으로 적용됩니다.

### 2. 복제할 카테고리 지정

`sync-categories`로 카테고리를 수집한 뒤, 복제할 카테고리에 `should_mirror = true`를 설정합니다.

```bash
blog-mirror sync-categories

psql $DATABASE_URL -c "SELECT category_no, name FROM categories ORDER BY category_no;"

# 원하는 카테고리 활성화
psql $DATABASE_URL -c "UPDATE categories SET should_mirror = true WHERE category_no IN (42, 57);"
```

### 3. 전체 게시물 초기 수집

```bash
blog-mirror init
```

### 4. 자동 동기화 시작

```bash
blog-mirror sync-loop
```

---

## 카테고리 표시 이름 커스터마이징

블로그 게시물 태그에 네이버 카테고리 원본 이름 대신 다른 이름을 사용하고 싶을 때는 `display_name`을 설정합니다.

```sql
UPDATE categories SET display_name = '원하는이름' WHERE category_no = 42;
```

`display_name`이 NULL이면 원본 `name`을 그대로 사용합니다.

---

## Docker

```bash
# 이미지 빌드
docker build -t blog-mirror .

# 실행 (sync-loop 모드, 기본 1시간 주기)
docker run --env-file .env -v /path/to/blog-clone:/blog blog-mirror

# 커스텀 주기
docker run --env-file .env -v /path/to/blog-clone:/blog blog-mirror sync-loop --interval 1800

# 초기화
docker run --env-file .env -v /path/to/blog-clone:/blog blog-mirror init
```

> `GITHUB_REPO_PATH`에 지정한 경로를 컨테이너 볼륨으로 마운트해야 합니다.

---

## 생성되는 파일 형식

게시물은 Zola 형식의 Markdown으로 저장됩니다.

```
+++
title = "게시물 제목"
date = 2024-01-15T10:30:00+09:00
[taxonomies]
tags = ["카테고리명"]
[extra]
naver_log_no = 224217407066
naver_category_no = 42
+++

본문 Markdown...
```

- 파일 위치: `{GITHUB_REPO_PATH}/content/blog/{log_no}.md`
- 이미지 위치: `{GITHUB_REPO_PATH}/static/images/img_{hash}.{ext}`
