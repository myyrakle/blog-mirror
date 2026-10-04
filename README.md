# blog-mirror

네이버 블로그 게시물을 자동으로 GitHub Pages(Zola) 블로그에 복제하는 도구입니다.

---

## 동작 방식

```
네이버 블로그 → (fetch) → PostgreSQL DB → (publish) → GitHub 블로그 저장소
                              ↑
                        웹 대시보드 (serve)
```

1. **fetch**: 네이버 블로그 API를 통해 신규 게시물 목록과 본문 HTML을 수집해 DB에 저장
2. **publish**: DB에 저장된 게시물을 Markdown으로 변환하고 GitHub 저장소에 커밋 & 푸시
3. **serve**: 위 과정을 주기 실행하면서, 브라우저에서 카테고리 설정·수동 동기화·재발행을 할 수 있는 관리 UI 제공

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

# ── 관리자 대시보드 (serve) ──
# 리스닝 포트 (기본값: 8080)
WEB_PORT=8080

# HTTP Basic 인증. 둘 다 설정해야 인증이 켜집니다.
WEB_USERNAME=admin
WEB_PASSWORD=change_me
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

> 관리 UI까지 함께 쓰려면 아래 [`serve`](#serve--관리자-웹-대시보드)를 쓰세요. `serve`가 이 동작을 포함합니다.

```bash
# 기본값: 3600초(1시간) 주기
blog-mirror sync-loop

# 커스텀 주기 (초 단위)
blog-mirror sync-loop --interval 1800
```

---

### `serve` — 관리자 웹 대시보드

브라우저에서 카테고리를 관리하고, 동기화를 수동으로 돌리고, 수정된 글을 다시 발행할 수 있는 웹 UI를 띄웁니다.

```bash
# 대시보드 + 1시간 주기 자동 동기화 (권장)
blog-mirror serve

# 포트 변경
blog-mirror serve --port 9000

# 자동 동기화 없이 대시보드만
blog-mirror serve --interval 0
```

기본 포트는 `8080`(환경변수 `WEB_PORT`)입니다.

`serve`는 `sync-loop`의 상위호환입니다. 주기 실행과 대시보드의 수동 실행이 **같은 작업 큐**를 쓰기 때문에
두 작업이 동시에 git 저장소를 건드리는 일이 없습니다. 따라서 `sync-loop`와 `serve`를 같이 띄우지 말고
`serve` 하나만 띄우면 됩니다.

#### 기능

| 탭 | 하는 일 |
|---|---|
| 대시보드 | 수집/복제 현황 통계, 수동 동기화 버튼, 실행 중인 작업의 실시간 로그 |
| 카테고리 | 복제 대상(`should_mirror`) 토글, 태그에 쓸 표시 이름(`display_name`) 편집, 일괄 설정 |
| 게시글 | 제목·logNo 검색, 카테고리/상태 필터, 변환된 마크다운 미리보기, 재동기화 |
| 작업 로그 | 과거 실행 이력과 각 실행의 전체 로그 |

#### 변경된 글 재동기화

네이버에서 글을 수정했다면 **게시글** 탭에서 해당 글을 골라 다시 발행할 수 있습니다.

- **네이버에서 다시 받아 재발행** — 본문 HTML을 네이버에서 새로 받아온 뒤 `.md`를 다시 쓰고 push 합니다.
- **저장된 본문으로 재발행** — 네이버를 다시 긁지 않고, DB에 있는 본문으로 `.md`만 다시 씁니다.
  (변환 로직을 고친 뒤 기존 글에 반영할 때 유용합니다.)

**카테고리** 탭에서 카테고리를 골라 그 안의 글을 통째로 재발행할 수도 있습니다.

> 복제 대상이 아닌(`should_mirror = false`) 카테고리의 글은 재동기화를 걸어도 발행되지 않습니다.
> 작업 결과 메시지에 몇 건이 그렇게 건너뛰어졌는지 표시됩니다.

#### 인증

`WEB_USERNAME`과 `WEB_PASSWORD`를 **둘 다** 설정하면 HTTP Basic 인증이 켜집니다.
하나라도 비어 있으면 대시보드는 인증 없이 열립니다. (`/healthz`는 항상 인증 없이 열려 있습니다.)

```env
WEB_USERNAME=admin
WEB_PASSWORD=change_me
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
```

가장 쉬운 방법은 대시보드(`blog-mirror serve`)의 **카테고리** 탭에서 토글을 켜는 것입니다.
psql로 직접 하려면:

```bash
psql $DATABASE_URL -c "SELECT category_no, name FROM categories ORDER BY category_no;"

# 원하는 카테고리 활성화
psql $DATABASE_URL -c "UPDATE categories SET should_mirror = true WHERE category_no IN (42, 57);"
```

### 3. 전체 게시물 초기 수집

```bash
blog-mirror init
```

### 4. 자동 동기화 + 대시보드 시작

```bash
blog-mirror serve
```

---

## 카테고리 표시 이름 커스터마이징

블로그 게시물 태그에 네이버 카테고리 원본 이름 대신 다른 이름을 사용하고 싶을 때는 `display_name`을 설정합니다.
대시보드 **카테고리** 탭의 '표시 이름' 칸에 입력하면 되고, SQL로는 다음과 같습니다.

```sql
UPDATE categories SET display_name = '원하는이름' WHERE category_no = 42;
```

`display_name`이 NULL이거나 비어 있으면 원본 `name`을 그대로 사용합니다.

---

## Docker

```bash
# 이미지 빌드
docker build -t blog-mirror .

# 실행 (기본: serve — 대시보드 + 1시간 주기 동기화)
docker run --env-file .env -p 8080:8080 -v /path/to/blog-clone:/blog blog-mirror

# 커스텀 주기
docker run --env-file .env -p 8080:8080 -v /path/to/blog-clone:/blog blog-mirror serve --interval 1800

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
