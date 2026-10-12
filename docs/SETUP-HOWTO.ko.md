# LLM용 Sacho 설정 가이드

이 문서는 실행 파일에 내장돼 있습니다. `sacho help setup --language ko` 또는
`sacho setup-howto --language ko`로 읽으세요. 설정 파일·서버·인터넷이 필요 없습니다.
일반 명령 도움말인 `sacho help serve`도 사용할 수 있습니다.

## LLM에 전달할 작업 지시

> `sacho help setup --language ko`를 읽고 이 설치를 설정하세요. 기존 설정과 API
> 상태를 먼저 확인하세요. 운영 주소·계정·검사 스크립트·비밀값·로컬 TODO는
> Git 밖에 보관하세요. 검사 ID와 이력을 유지하고, 저장 전에 초안을 테스트하세요.
> 서비스별 클라이언트를 본체에 추가하지 말고 외부 시나리오를 사용하세요.
> 명시적인 요청이 없으면 GitHub Actions를 실행하지 말고 로컬에서 검증하세요.
> 결과·검증·미확정 입력을 보고하되 비밀값은 출력하지 마세요.

## 먼저 확인할 정보

실행 파일·버전·서비스 실행 방식·설정 경로·DB·공개 URL·프록시·비밀값 출처를
확인합니다. 관리자 이메일, 검사 대상 URL/쿼리/정상 조건/주기, SMTP 발신자와
릴레이·보안 방식, 웹훅과 업데이트 정책 중 모르는 값만 운영자에게 확인합니다.
기존 설치 전체를 덮어쓰거나 인증 정보를 추측하지 마세요.

UI·Lua·HTTP/JSON·SQLite·이 가이드와 암호화 기능은 실행 파일에 내장됩니다.
Python·쉘·psql은 해당 검사에서 사용하는 외부 프로그램입니다. HTTP/JSON/env/exec
사용에 Lua 패키지를 추가 설치할 필요는 없습니다. 아래 주소와 경로는 예시입니다.

```sh
sacho --version
umask 077
# 신규 로컬 설치에서만 실행. 기존 파일은 덮어쓰지 않습니다.
sacho init --config ./sacho.toml
sacho doctor --config ./sacho.toml
sacho serve --config ./sacho.toml
```

기본값은 루프백 127.0.0.1:8080/no-auth이며 `/`와 `/admin`을 로컬에서 엽니다.
doctor는 설정·등록 검사·SQLite 무결성을 검사하고 DB 생성/마이그레이션도 할 수
있습니다. 기존 설치는 실제 설정과 GET `/api/admin/items`부터 확인하세요.
운영에서는 절대 경로를 사용하고 설정과 SQLite를 일관되게 백업하세요.
SQLite backup API 또는 서비스 중지 후 복사를 사용합니다.

## 외부 설정

```toml
bind = "127.0.0.1:8080"
database = "/var/lib/sacho/sacho.db"
public_url = "https://status.example.com"
retention_days = 100
max_concurrent_checks = 16
# 최상위 항목. [auth] 등 섹션보다 앞에 둡니다.
probe_env = ["SOURCE_BEARER", "CHECK_CONTEXT"]

[auth]
mode = "auth-proxy"
email_header = "X-Forwarded-Email"
admins = ["operator@example.com"]
# 지원되는 프록시 로그아웃 경로가 있을 때만 지정합니다.
logout_url = "/identity/logout"

[updates]
enabled = false
interval_seconds = 21600

# 이메일 구독이 필요할 때 활성화합니다.
# [smtp]
# host = "smtp.example.com"
# port = 587
# from = "Status <status@example.com>"
# username = "status@example.com"
# password_env = "SMTP_PASSWORD"
# security = "starttls"
```

외부 설정을 바꾼 뒤 serve를 재시작합니다. 관리자 설정 API는 공개 제목·설명만
변경하며 인증·SMTP·환경변수를 변경하지 않습니다. SMTP는 starttls/tls를 지원하고
plain은 루프백/사설 IPv4 릴레이만 허용합니다. SMTP 비밀번호는 probe_env에
등록할 필요가 없습니다. SMTP 미설정 시 구독은 비활성화되고, 설정 후에도
이메일 확인을 완료해야 구독 상태가 됩니다.

`/admin`, `/api/admin/`, `/api/account`에는 기존 인증 프록시를 적용하세요.
클라이언트의 신원 헤더를 제거한 뒤 검증된 이메일을 주입해야 합니다. 공개
상태·이력·장애 API는 익명 접근입니다. Hopae는 애플리케이션 호스트의 `/_hopae`도
보호해야 로그인 핸드오프/로그아웃이 동작하며 logout_url은 `/_hopae/logout`입니다.
기존 인증 계약을 재사용하고 비밀값을 문서에 복사하지 마세요. 공개 접근,
비로그인 차단, 위조 헤더 차단을 각각 확인합니다. 프록시 뒤 원본 리스너는
외부에 노출하지 않습니다. no-auth는 루프백 로컬 작업용입니다.

## 환경변수·추가 정보·암호화된 입력 주입

비밀이 아닌 입력은 저장소 밖 서비스 EnvironmentFile에 넣을 수 있습니다.

```text
CHECK_CONTEXT='{"environment":"staging","region":"example"}'
```

필요한 변수 이름을 최상위 probe_env에 등록하고 Lua에서 `sacho.env("NAME")`으로
읽습니다. 구조화된 값은 `sacho.json_decode(sacho.env("CHECK_CONTEXT"))`로 읽습니다.
HTTP headers_env에는 값 대신 변수 이름을 매핑합니다. Authorization 변수 값은
필요하면 `Bearer ` 접두사까지 포함한 전체 헤더여야 합니다. 외부 명령도 허용한
변수와 고정 PATH/LANG만 받습니다. 허용 목록은 전체 설치에 공통이며 신뢰된
관리자 사이의 개별 권한 경계는 아닙니다.

비밀값이 파일에 평문으로 남지 않아야 하면 암호화 저장소를 사용하세요.
부모 디렉터리를 먼저 만들고 서비스 사용자 소유/0700 권한으로 보호합니다.

```sh
sacho secrets init --vault /etc/sacho/secrets.enc --key-file /etc/sacho/unlock.key
credential-reader | sacho secrets set SOURCE_BEARER --vault /etc/sacho/secrets.enc --key-file /etc/sacho/unlock.key
sacho secrets list --vault /etc/sacho/secrets.enc --key-file /etc/sacho/unlock.key
```

credential-reader는 운영자의 기존 비밀값 공급 방식에 대한 자리 표시자입니다.
내장 명령이 아닙니다. 비밀값을 argv·쉘 이력·로그·Git에 넣지 마세요.

```toml
[secrets]
vault = "/etc/sacho/secrets.enc"
key_file = "/etc/sacho/unlock.key"
```

JSON 맵은 인증된 암호화로 저장하며, 키/암호문 파일은 비공개 권한이어야 합니다.
키는 암호가 아닌 무작위 키 재료입니다. 키를 별도로 보호하고 둘 다 안전하게
백업하세요. 저장된 값의 평문 검색을 막지만 두 파일이나 프로세스 메모리를
읽을 권한까지 막지는 않습니다. 시작 시 메모리에서 복호화하고 probe_env에
등록한 이름만 검사에 전달합니다. SMTP 비밀번호·HTTP 헤더도 같은 이름 조회를
사용합니다. 실제 서비스 환경변수 값이 vault 항목보다 우선하며 변경 후에는
재시작이 필요합니다. list는 이름만 출력합니다. 값 자체를 출력하지 마세요.

## 검사 등록 API

프록시의 정상 로그인 세션을 사용합니다. 로컬 운영자는 비공개 루프백 원본에
설정된 관리자 헤더를 전달할 수 있지만 공개 URL에 헤더만 추가해 인증할 수는
없습니다. 변경 요청은 X-Sacho-Request: 1이 필수이고 Origin이 있으면 public_url과
일치해야 합니다. JSON 본문은 Content-Type: application/json입니다.

| 작업 | 메서드·경로 | 계약 |
| --- | --- | --- |
| 목록 | GET /api/admin/items | id/spec/status/revision 등이 담긴 배열 |
| 초안 검사 | POST /api/admin/test | CheckSpec 입력. status/latency_ms/detail/output/display_value 반환. 기록·장애·알림은 저장하지 않지만 스크립트는 실행됨 |
| 생성 | POST /api/admin/items | CheckSpec 입력, id 반환 |
| 수정 | PUT /api/admin/items/{id} | 전체 CheckSpec. 변경하지 않는 필드는 기존 spec에서 보존 |
| 저장된 검사 실행 | POST /api/admin/items/{id}/run | 검사 예약. 기록·알림이 발생할 수 있음 |
| 로그 | GET /api/admin/items/{id}/logs | 원본 출력은 비공개 유지 |
| 삭제 | DELETE /api/admin/items/{id} | 해당 검사 기록도 삭제. 이전 작업에는 사용하지 않음 |
| 제목·설명 | PUT /api/admin/settings | title/description |
| 레이아웃 | PUT /api/admin/layout | sections 배열의 title/item_ids |
| 웹훅 | GET/POST /api/admin/webhooks | 생성 본문 name/url, /{id}에 DELETE |
| 알림 기록 | GET /api/admin/notifications | 전송·재시도 결과 |

수정 API에는 클라이언트 revision/CAS 조건이 없으므로 동시 설정 작업을 피하세요.
기존 ID를 재사용하고, 이름이 중복이면 운영자 의도를 먼저 확인합니다.
알 수 없는 필드 이름은 오류입니다.

로컬 상태 프로세스 자체를 검사하고 버전을 표시하는 예입니다.

```json
{
  "name":"Status process", "group":"Infrastructure", "kind":"http",
  "target":"http://127.0.0.1:8080/healthz",
  "interval_seconds":60, "timeout_seconds":10, "expected_status":200,
  "enabled":true, "publish_value":true, "display_pointer":"/version",
  "display_label":"Version", "metric_unit":"", "threshold":null
}
```

먼저 초안 검사가 operational인지 확인한 뒤 생성/수정합니다. 저장된 검사의 새
관측과 공개 표시값까지 확인하세요. queued 응답만으로 성공을 판정하지 않습니다.

CheckSpec: name은 필수/120바이트, description 2000, group 120, target은 필수/64KiB.
kind는 http/shell/python/lua, interval_seconds는 5~86400, timeout_seconds는
1~300이며 검사 주기를 넘으면 안 됩니다. expected_status 기본 200,
expected_exit 기본 0, contains는 선택 문자열입니다. headers_env는 허용 변수 이름
매핑, json_pointer/comparison/threshold는 숫자 조건입니다. 비교는 lt/lte/gt/gte/eq,
threshold null은 숫자 판정 없음입니다. degraded_ms는 선택 응답 지연 기준,
failure_threshold/recovery_threshold는 1~100 연속 횟수이며 기본 1입니다.
enabled 기본 true, publish_value 기본 false입니다. display_pointer/display_label/
metric_unit은 상태 조건과 독립적인 표시 설정입니다.

TPS JSON의 /tps와 단위 ` TPS`, version 문자열의 /version을 표시할 수 있습니다.
숫자 조건이 필요하면 json_pointer /tps, comparison gte, threshold 10 같은 식으로
별도 지정합니다. 없는 값을 0이나 잔여량 100%로 만들어내지 마세요.

## Lua·Prometheus·SQL 예제

실행 파일에서 예제 소스를 꺼낼 수 있습니다. 저장소 checkout이 필요 없습니다.

```sh
sacho template prometheus
sacho template postgresql
sacho template external-command
```

Prometheus는 PROMETHEUS_URL/PROMETHEUS_QUERY를 주입하고 Lua 검사로 등록합니다.
순간 조회 API의 scalar 또는 정확히 한 개의 float vector 샘플을 읽습니다.
빈 값·복수 시리즈·비유한 값·히스토그램은 이 예제에서 실패로 처리합니다.
`sum(rate(http_requests_total[5m]))` 같은 집계나 명시적인 라벨 선택을 사용하세요.
결과 /value를 표시하거나 정상 조건으로 비교합니다. 데이터 최신성은
`time()-timestamp(metric)` 같은 별도 쿼리로 검사하세요. 조회 평가 시각이 최신
스크레이프의 증명은 아닙니다. 주기적 관측이며 과거 Prometheus 기록 전체를
가져오거나 remote-write를 수신하는 기능은 아닙니다.

PostgreSQL은 서비스 호스트에 psql을 설치하고 PGHOST/PGPORT/PGDATABASE/PGUSER/
PGPASSFILE/PGCONNECT_TIMEOUT/PGSQL_QUERY를 주입합니다. PGSQL_QUERY는
`SELECT json_build_object('value',1,'version',version())`처럼 value 필드가 스칼라인 JSON 객체 하나를
반환하는 SELECT입니다. 읽기 전용 DB 계정을 쓰세요. 예제는 READ ONLY 트랜잭션,
5초 쿼리 제한, 비대화형 인증, psqlrc 차단을 사용합니다. 비밀번호 파일은 비공개로
두거나 vault의 PGPASSWORD 항목을 사용할 수 있습니다. 원본 행보다 필요한
집계만 공개하세요. 다른 SQL 엔진도 해당 외부 클라이언트로 같은 계약을 적용합니다.
본체에 PostgreSQL 드라이버나 psql을 내장하지는 않습니다.

HTTP/JSON/env/exec는 내장 API입니다. exec는 literal argv를 받아 쉘 확장을 하지
않고 스트림별 64KiB만 캡처합니다. 시간 초과·취소 시 검사 프로세스 그룹을
정리합니다. HTTP 본문은 64KiB, 리다이렉션 없음, TLS 검증 적용입니다.
표준 Lua io/os/package/debug·동적 모듈은 제공하지 않습니다. 외부 프로그램의
종료 코드가 0인지 검사한 뒤 stdout JSON만 반환하세요. 비밀 context나 stderr는
공개하지 마세요. 서비스별 로직은 본체가 아닌 외부 시나리오에 둡니다.

## 이메일·웹훅 템플릿

관리자 → 알림에서 템플릿을 편집하고 저장 전에 미리보세요. MiniJinja 문법으로
`{{ status_name }}`은 값 삽입, `{% if recovered %}`는 조건문입니다. 메일은 일반
텍스트이며 웹훅은 JSON 객체여야 합니다. 문자열은 tojson 필터로 이스케이프합니다.
webhook_body를 비우면 기본 이벤트 페이로드를 유지합니다.

```jinja
{{ site_title }} · {{ service_name }} · {{ status_name }}
{% if recovered %}복구{% else %}장애 감지{% endif %}: {{ service_name }}
{% if display_value %}{{ display_label }}: {{ display_value }}{{ metric_unit }}{% endif %}
{{ time }} · {{ public_url }}
```

```jinja
{"text": {{ (service_name ~ ": " ~ status_name) | tojson }}, "event": {{ event | tojson }}}
```

GET /api/admin/templates는 templates/defaults/variables를 반환합니다.
PUT은 subject/body/verification_subject/verification_body/webhook_body가 담긴
전체 templates 객체를 받습니다. webhook_body null은 기본 웹훅입니다.
POST /api/admin/templates/preview에 templates와 event를 넣으면 저장·전송 없이
가상 데이터로 미리봅니다. incident.started/incident.resolved/subscription.verify를
각각 확인하세요. 모르는 변수, 잘못된 JSON, 여러 줄 제목, 크기·실행 한도 초과는
거부합니다. 사용자 본문에 구독 확인/해지 링크가 없으면 자동 추가합니다.
전송 대기·재시도에도 현재 템플릿을 적용합니다.

변수: site_title/public_url/service_name/description/group/status/status_name/event/
event_name/recovered/incident_id/item_id/delivery_id/at/time/display_value/
display_label/metric_unit/latency_ms/confirm_url/unsubscribe_url.
at는 Unix 초, time은 UTC ISO입니다. 선택 표시값이 없거나 오래된 대기 이벤트에
메타데이터가 없으면 빈 값을 사용합니다. 템플릿에는 인증값·환경변수 조회·원본
검사 출력·파일/네트워크 로더를 제공하지 않습니다. 미리보기는 실제 구독 토큰을
사용하지 않습니다.

## 완료 검증·자동 갱신·문제 해결

healthz/공개 api/status/새 검사 로그/표시값을 확인합니다. 여러 관측 후 차트도
확인하세요. 회색은 데이터 없음이고, 조회 기간에 맞는 시간 간격만 지원합니다.

상태 화면은 약 30초마다 상태·장애·차트를 재조회하며 데이터 대기 상태는
5초 간격입니다. 숨겨진 탭과 열린 대화상자에서는 주기 조회를 멈춥니다.
탭 복귀·네트워크 복구 시 대화상자가 없으면 즉시 재조회합니다. 기간/시간 간격을
유지하고 브라우저 단조 시계로 예약합니다. 실패 시 오래된 데이터 안내를 표시하고
재시도하며 GET은 15초 제한입니다. 이는 각 검사의 실행 주기와 별개입니다.
이메일·구독 상태도 30초 및 탭/네트워크 복귀 시 갱신합니다.

SMTP는 지정된 테스트 수신함/릴레이로 구독 확인과 장애 시작/복구 각 1회 전송을
검증합니다. 초안 테스트로는 영속 알림 전달을 검증할 수 없습니다. 운영 검사를
이메일 테스트만을 위해 장애로 만들지 마세요.
401은 신원/프록시, 403은 변경 헤더/Origin, 400은 필드/설정, 429는 동시 초안
제한을 확인합니다. 변수는 주입과 probe_env 등록 모두 필요합니다. SQL 클라이언트와
접속 정보는 대화형 쉘이 아닌 서비스 환경에서도 사용 가능해야 합니다.
변경 후 그 환경에서 doctor/재시작을 수행하고 되돌릴 백업을 보존합니다.
자동 업데이트는 선택 사항이며 감독 프로세스를 사용하는 serve로 실행합니다.

완료 보고에는 접근 상태, 검사 ID와 새 관측, 표시/정상 판정 의미, 변수 이름,
SMTP/업데이트 활성 여부, 외부 의존성, 로컬 검증과 미확정 입력만 담습니다.

참고: [Prometheus 조회 API](https://prometheus.io/docs/prometheus/latest/querying/api/),
[psql](https://www.postgresql.org/docs/current/app-psql.html),
[libpq 환경변수](https://www.postgresql.org/docs/current/libpq-envars.html),
[RustCrypto 암호화](https://docs.rs/chacha20poly1305/0.10.1/chacha20poly1305/).

개발 빌드의 `sacho help storage --language ko`에는 백업·복원·SQLite/PostgreSQL
이전 안내가 임베딩되어 있습니다. `sacho database export --database ./data.db
--output ./backup.json`, `database verify --input ./backup.json`, `database import
--input ./backup.json --database ./restored.db`로 검증합니다. 스냅샷은 구독 토큰을
포함하는 비공개 평문이며 0600으로 생성하고 기존 파일을 덮어쓰지 않습니다.
Git·공개 경로에 넣지 마세요. `import-postgres`/`export-postgres`는 환경변수
SACHO_DATABASE_URL로 접속합니다. 실행 저장소는 아직 SQLite이므로 운영 설정을
PostgreSQL로 바꾸거나 분산 워커를 활성화하면 안 됩니다.

제품 범위는 모니터링·현재 값과 이력·신뢰할 수 있는 알림 전달로 제한합니다.
장애 관리·온콜·대응은 외부 웹훅 수신 시스템의 업무입니다. 기존 incident.* 이벤트와
incident_id는 장애·복구 신호이며 인시던트 관리 구현 요구가 아닙니다.
명시적인 추가 요청 없이 일반 기업용 업무 시스템으로 확장하지 마세요.

원격 검사는 SQLite 중앙 하나와 아웃바운드 `sacho worker` 프로세스로 실행합니다.
`sacho help workers --language ko`로 전체 안내를 보고 `sacho template remote-worker`로
예제 설정을 출력합니다. remote_workers에 노드별 토큰 참조를 등록하고 검사 편집기에서
워커 ID를 지정합니다. 스크립트는 양쪽의 허용이 필요하며 입력은 워커 로컬에 둡니다.
원격 초안 테스트는 지원하지 않고 저장한 검사를 중앙에서 대신 실행하지 않습니다.
여러 중앙 서버의 HA는 미룹니다.

원격 에이전트의 권장 등록은 관리자 에이전트 탭에서 노드 지정 일회용 키를 발급하고
worker-join의 표준입력으로 전달하는 방식입니다. 자체 인증정보는 워커 암호화
저장소에 보관하고 중앙에는 해시만 저장합니다. 응답 손실은 키 재사용 없이 저장된
인증정보로 복구합니다. 에이전트 선택·철회는 관리자에서 처리합니다. 자세한 절차는
`sacho help workers --language ko`에 임베딩되어 있습니다.

Docker 모니터링: `sacho template docker`는 Unix 소켓 GET 조회를 사용하는 내장 Lua
예제를 출력합니다. 선택한 실행 노드에 `DOCKER_SOCKET`, `DOCKER_CONTAINER`를
인젝션하고 `probe_env`에 허용하세요. 조건은 `/healthy gte 1`, 공개 조회값은
`/display`, 공개 설명 `public_message_pointer`는 `/message`로 설정합니다.
Docker CLI는 필요 없습니다. 소켓 자체는 읽기 전용 마운트에서도 호스트 제어 권한을
주므로 신뢰한 검사에만 허용하세요. 조회 실패 시 이전 수치는 시각과 함께 남고,
이전 값도 없으면 조회 불가로 표시합니다. 선택한 설명만 공개하며 내부 오류와
컨테이너 환경변수 원문은 공개하지 않습니다.
잔량·수집기 검사는 `failure_status="degraded"`로 지정하면 조건 실패·오류·시간 초과를
주의로 표시합니다. 기본값은 `outage`입니다. 새 선택 필드는 최신 워커와 함께 쓰세요.

관리자 수치 임계값·추이·신선도·워커 하트비트·검사 템플릿·안전한 설정 전달·알림 억제는
`sacho help monitoring --language ko`의 내장 가이드로 설정할 수 있습니다.

추가 호스트는 프록시 라우트와 인증 앱에 함께 등록하세요.
`public_origins=["https://monitor.example.com"]`로 추가 Origin의 관리자 저장·구독
요청을 허용합니다. 메일 링크와 등록 메타데이터의 기본 주소는 `public_url`입니다.
최대 16개의 정확한 HTTP(S) Origin만 허용하며 와일드카드·임의 Host·전달 헤더로
권한을 넓히지 않습니다. 변경 후 서비스를 다시 시작하세요.

## 모니터링 확장 설정 (v0.1.9 이상)

입력·검사별 동작·cron/시간대·네이티브 Prometheus/DNS/TCP·인증된 외부 보고를 설정하기
전에 `sacho help monitoring --language ko`를 읽으세요. 관리자 검사 템플릿에 Shell/Python/Lua와
내장 소스 예제가 있으며 환경변수·비밀값에서 이름만 조회하고 암호화 값을 교체할 수 있습니다.
내장 네트워크 검사는 선택한 최신 에이전트에서 실행하고, 검사별 동작은 메인 서버에서 실행합니다.
동작 기본값은 없음이며 지연·재시도·쿨다운·실행 후 대기를 지정할 수 있습니다. Push 보고 키는
재사용 가능하며 에이전트 일회용 등록 키와 별개입니다. 문서에 없는 프로토콜 지원을 가정하지 마세요.
