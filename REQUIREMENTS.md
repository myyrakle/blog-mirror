# 블로그 미러 시스템

- Naver Blog에 저장된 게시글을 다른 블로그 (Github Blog)로 자동 복제하기 위한 시스템입니다.
  기본 입력 파라미터

1. Naver 블로그 ID
2. Github Blog 레포지토리
3. Github Blog UserName/Password

## 기술 스택

1. Rust
2. PostgreSQL
3. Tokio

## 최초 동기화 프로세스 (수동 트리거)

1. Naver Blog를 전체 스캔합니다.
2. 카테고리 목록을 수집해서 저장합니다. (DB에 저장)
3. 모든 글 목록을 조회해서 수집합니다. (DB에 저장)

## 주기적 동기화 프로세스 (1시간마다)

1. 새로 추가된 카테고리가 있다면 수집해서 저장합니다. (DB에 저장)
2. Naver Blog를 주기적으로 스캔하고, 새로 등록된 글을 수집합니다. (내림차순이기 때문에 마지막으로 읽고 처리한 ID를 커서로 저장하고 이를 기반으로 멈추면 됩니다.)

## 복제 프로세스 (1시간마다)

1. 수집된 글 중에서 특정 카테고리에 속하고, 아직 복제되지 않은 글을 Github Blog로 마이그레이션합니다.
2. 마이그레이션이 완료된 글을 복제 완료된 것으로 상태를 변경합니다.

## Naver 크롤링

- API를 훔쳐서 사용할 수 있습니다. 차단에 걸릴 수 있으므로 적당히 간격을 두는 편이 좋습니다.

### List 조회

- List API를 사용하면 바로 리스트를 최신순 (등록 내림차순)으로 조회할 수 있습니다. currentPage은 1부터 시작합니다.
- https://blog.naver.com/PostTitleListAsync.naver?blogId=[ID]&viewdate=&currentPage=[번호]&categoryNo=0&parentCategoryNo=0&countPerPage=30
  예시: https://blog.naver.com/PostTitleListAsync.naver?blogId=sssang97&viewdate=&currentPage=1&categoryNo=0&parentCategoryNo=0&countPerPage=30

### 상세조회

- Get API를 사용하면 바로 HTML을 받아올 수 있습니다.
- https://blog.naver.com/PostView.naver?blogId=sssang97&logNo=[상세번호]&redirect=Dlog&widgetTypeCall=true&noTrackingCode=true&directAccess=false
  예시: https://blog.naver.com/PostView.naver?blogId=sssang97&logNo=224217407066&redirect=Dlog&widgetTypeCall=true&noTrackingCode=true&directAccess=false

## Github Blog 형식 변환

- Zola, terminus, Github Pages를 사용해서 관리하고 있습니다.
- 그래서 게시하려면 Github에 commit하고 push를 하는 방식이어야 합니다.
- 글은 /content/blog/[logNo].md의 형태로 만들고, title은 원본을 그대로 계승합니다.
- 글에 이미지가 포함되어있다면 /static/images에 업로드하고 그것의 참조로 대체합니다.
- Naver 글 본문은 자체 포맷으로 정의되어있습니다. 해당 포맷을 자체 마크다운 표현으로 변환합니다.
- Naver 글 본문에 코드 삽입이 있다면 적절한 마크다운 표현으로 치환합니다.
