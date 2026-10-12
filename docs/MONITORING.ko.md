# 모니터링 설정

`sacho help monitoring --language ko`로 오프라인에서도 읽을 수 있습니다.
컨트롤러와 에이전트는 같은 바이너리를 사용합니다. 새 선택 필드는 에이전트를
업데이트한 뒤 사용하세요.

## 수치 주의·위험 임계값

관리자 → 모니터 → 편집에서 등급 판정 JSON 포인터, 위험 방향, 주의·위험 값을
설정합니다. 임계값을 모두 비우면 비활성화됩니다. 경계값도 포함합니다.

```json
{
  "value_rules": {
    "pointer": "/remaining_percent", "direction": "low",
    "warning": 20, "critical": 5
  },
  "publish_value": true, "display_pointer": "/display",
  "publish_trend": true, "trend_pointer": "/remaining_percent",
  "failure_status": "degraded"
}
```

잔량은 20 이하 주의, 5 이하 위험입니다. `high`는 CPU·지연 시간처럼 높을수록
위험한 수치에 씁니다. 위험 임계값은 주의보다 심각해야 합니다. 기존 HTTP·종료
코드·문자열·숫자 조건도 유지하며 다른 실패를 정상으로 바꾸지 않습니다.
포인터 누락·숫자가 아닌 값은 `failure_status`로 실패하고 관리자 설명에 남습니다.
최종 등급에는 기존 연속 실패·연속 복구 조건이 적용됩니다. 수치는 계속 표시합니다.

## 추이와 데이터 신선도

`publish_trend=true`일 때만 수치 이력을 공개합니다. `trend_pointer`를 지정하세요.
비우면 등급 포인터 → 표시 포인터 → 기존 조건 포인터 순서로 사용합니다.
모니터 하나당 숫자 시계열 하나를 표시하며 문자열 요약과 숫자 그래프는 별도로
선택할 수 있습니다. 구간 평균, 후버·키보드 포커스에서 최솟값·최댓값, 임계선을
표시합니다. 선택기로 응답 시간 추이도 볼 수 있습니다. 값이 없는 구간은 빈칸이고
0으로 채우지 않습니다. 조회 기간·지원 해상도는 기존 타임라인과 연동합니다.
숫자 포인터를 바꾸면 다른 의미의 수치가 섞이지 않도록 과거 숫자 샘플만 비우고
검사 출력과 가용성 이력은 보존합니다. 새 정의의 그래프는 다음 검사부터 시작합니다.

카드에 최신 관측, 마지막 정상 검사, 다음 검사 예정 시각을 표시합니다. 예정 시각은
추정이며 대기열·워커 상황에 따라 늦어질 수 있습니다. 오래된 데이터와 이전
조회값은 명시하고, 보존된 수치에는 실제 수치를 얻었던 시각을 표시합니다.

## 에이전트 상태

관리자 → 에이전트에서 연결 상태, 마지막 접속, 보고한 바이너리 버전, 실행 중인
검사를 봅니다. 인증된 하트비트를 10초마다 보내며 긴 검사를 실행하는 동안에도
계속됩니다. 마지막 접속이 30초를 넘으면 연결 끊김, 접속 이력이 없으면 확인 대기,
철회한 노드는 철회됨으로 표시합니다. 워커 단절만으로 대상 서비스 이력을 장애로
바꾸지 않습니다. 버전은 워커가 보고한 정보이며 암호학적 증명은 아닙니다.
구형 워커도 작업 요청은 가능하지만 실행 중 하트비트를 보내지 못하므로 업데이트가
필요합니다.

## 설정 검증·예제·내보내기

관리자 → 검사 템플릿에서 HTTP·JSON/TPS·Prometheus·PostgreSQL·Docker를 선택합니다.
조건·표시 설정·필요한 입력을 가진 초안만 열고 자동으로 실행하거나 저장하지
않습니다. 실행 노드에 입력을 인젝션하고 그 노드의 `probe_env`에서 허용하세요.
PostgreSQL은 외부 `psql`이 필요하고, 나머지 Lua HTTP 조회는 내장되어 있습니다.

`required_env`는 API에서 문자열 배열, 편집 화면에서 쉼표로 구분한 이름입니다.
설정 검증은 실행 없이 규격·Lua 구문·등록된 에이전트·로컬 필수 입력을 확인합니다.
일반적인 `sacho.env(...)` 리터럴과 HTTP 헤더 참조도 확인하지만 동적인 이름은
명시적으로 선언하세요. 누락 입력은 경고로 표시하여 인젝션 전에 설정을 준비할 수
있습니다. 원격 입력은 컨트롤러 환경이 아니라 해당 에이전트에서 실행할 때 확인합니다.
시나리오 테스트로 로컬 JSON 포인터와 실제 조회 결과를 확인하세요. 저장·알림 없이
실행합니다. 저장 시 잘못된 규격과 Lua 구문은 거부합니다.

관리자 → 설정 → 안전한 설정 내보내기는 `sacho-monitor-metadata.json`을 내려줍니다.
모니터 메타데이터, 입력 이름, 레이아웃, 알림 정책을 포함합니다. 비밀값을 포함할 수
있는 모든 스크립트·대상 URL, 서버 인증·SMTP, 환경변수·암호화 금고의 값, 워커
인증, 구독자 주소, 웹훅 URL, 알림 템플릿은 제외합니다. 실행 가능한 전체 백업이나
자동 임포트 파일이 아닌 설정 전달용 메타데이터입니다. 대상 서버의 소스·운영 입력은
별도로 안전하게 공급하세요. 전체 백업은 금고와 비공개 SQLite/PostgreSQL 스냅샷
절차를 사용합니다.

보호된 관리자 API는 `GET /api/admin/examples`, `POST /api/admin/validate`
(CheckSpec JSON), `GET /api/admin/export`,
`GET|PUT /api/admin/notification-policy`입니다. 공개 상태 API는 스크립트·입력
이름·비공개 로그를 노출하지 않습니다.

## 알림 억제

관리자 → 알림에서 영구 저장되는 정책을 설정합니다.

```json
{"settle_seconds": 30, "cooldown_seconds": 300, "maintenance_until": 0}
```

- `settle_seconds`: 0~3600초. 문제 알림을 지연합니다. 전송 전에 같은 문제가
  복구되면 문제·복구 알림을 억제하고 이력은 유지합니다.
- `cooldown_seconds`: 0~86400초. 같은 모니터·수신 대상의 마지막 문제 알림 전송부터
  간격이 지나도록 반복 문제 알림을 지연합니다. 문제 알림을 받았다면 복구 알림은
  즉시 전송할 수 있습니다.
- `maintenance_until`: 최대 30일 뒤의 서버 Unix 시각. 모니터링 이메일·웹훅만
  중지하고 검사·상태·이력·구독 확인 메일은 유지합니다. 관리자 입력은 브라우저
  현지 시각이며 유지보수 종료 버튼으로 해제합니다.

지연한 문제 알림은 전송 시점의 등급과 공개 허용한 최신 수치를 사용합니다.
수치 공개를 끄면 대기 중인 문제 알림에서도 수치를 제외합니다.
기본값은 모두 0으로 즉시 전송을 유지합니다. 모니터별 연속 실패·복구 조건도
함께 사용하세요. 억제한 전송은 전송 로그에 이유와 함께 남으며 전송 성공으로
세지 않고 유지보수 종료 후 재전송하지 않습니다. 억제된 문제의 복구만 따로 보내지
않습니다. 유지보수 중 시작한 문제를 종료 시 자동 재통보하지 않습니다.
담당자·인지·에스컬레이션·온콜·조치·인시던트 관리 기능은 만들지 않습니다.
그 역할은 외부 웹훅 시스템에 맡깁니다. 전송 직전에 상태를 확인하지만 이미 진행
중인 외부 전송을 회수할 수는 없으며 전달 보장은 at least once입니다.

## 실행 예제와 관리자 환경변수·비밀값

**검사 템플릿**에 Shell script, Python code, Lua script 시작 예제,
네이티브 ICMP/SNMP/Prometheus/DNS/TCP 및 외부 보고가 있습니다. 선택하면 초안만 열리고,
**초안 테스트**는 검사만 실행하며 **저장**해야 스케줄이 활성화됩니다.
체크아웃 없이 `sacho template shell`, `python`, `lua`, `ping`으로 예제를 꺼낼 수 있습니다.
Python은 실행 노드의 `python3`, Shell은 `/bin/sh`, 네이티브 ping은 ICMP 소켓 권한이
필요합니다. Lua와 네트워크 프로토콜은 내장됩니다. ICMP 차단과 HTTP 서비스 장애는 별개입니다.

관리자의 **환경변수·비밀값**은 변수 이름, 주입 여부, 암호화 값 저장 여부와 출처만
보여줍니다. 암호화 비밀값은 다시 반환하지 않으며, 일반값은 평문 조회·편집이 가능합니다. `sacho secrets init --vault
/private/inputs.enc --key-file /private/unlock.key`로 초기화하고 `[secrets]`를 설정해
한 번 재시작하면 관리자 편집이 활성화됩니다. 암호화 파일과 잠금 해제 키는 별도로
백업하세요. 값 등록·교체·주입 해제·삭제는 다음 검사부터 재시작 없이 반영됩니다.
이미 실행 중인 프로세스가 받은 입력은 바뀌지 않습니다.

관리자에서 새 이름의 주입을 허용하면 허용 이름은 SQLite에, 암호화 비밀값은 vault에, 명시적으로 선택한 일반값은 SQLite에
저장됩니다. 서비스 환경변수가 vault보다 우선합니다. 서비스가 상속한 임의 환경변수를
관리자에서 바로 공개할 수는 없으며, 환경변수 입력은 서버 `probe_env`에 명시해야 합니다.
PATH, LANG, SACHO_PROBE_*와 설정된 에이전트 인증·SMTP 비밀번호는 관리 대상에서
제외합니다. 서버 설정에 있는 허용 이름은 설정에서 변경해야 합니다. vault 값을
삭제해도 같은 이름의 시스템 환경변수가 있다면 그 값은 남습니다.
원격 에이전트는 자체 허용 목록과 vault를 사용하고 메인 서버가 비밀값을 배포하지 않습니다.

Shell은 `"$API_TOKEN"`, Python은 `os.environ["API_TOKEN"]`, Lua는
`sacho.env("API_TOKEN")`으로 읽습니다. 스크립트·URL·동작 JSON에 비밀값을 직접 적거나
출력하지 마세요. 안전한 메타데이터 내보내기는 스크립트, 동작, PromQL, DNS 설정,
비밀값을 제외합니다. 전체 DB 백업은 비공개 운영 데이터입니다.

## 네이티브 검사

- **Prometheus**: `kind="prometheus"`, `target`은 서버 기본 URL, `prometheus_query`는
  PromQL입니다. `/api/v1/query` 순간 쿼리로 단일 scalar 또는 단일 vector 시리즈를
  받아 `{"value":42.0,"sample_at":...}`로 정규화합니다. `/value`를 최신 값·임계값·추세에
  연결하세요. 여러 시리즈는 `sum(rate(http_requests_total[5m]))`처럼 집계합니다.
  빈 결과·다중 시리즈·NaN/Inf는 실패입니다. 인증은 `headers_env`로 주입합니다.
  기존 Prometheus를 조회하며 PromQL 엔진·스크레이퍼·TSDB 자체를 내장하지 않습니다.
- **DNS**: `kind="dns"`, `target`은 도메인입니다. A/AAAA/CNAME/TXT/MX/NS/SOA/SRV/PTR/CAA를
  지원하며 `dns_server`는 선택적인 IP 또는 IP:port입니다. IPv6 포트는 대괄호를 씁니다.
  비워두면 시스템 DNS 서버를 쓰며 매 검사마다 새로 조회하고 hosts 파일·자체 캐시는
  쓰지 않습니다. `dns_expected`는 순서와 무관한 기대 응답 집합이고 비우면 비어 있지 않은
  응답을 허용합니다. DNS 표기 형식으로 도메인 끝의 점, MX/SRV 숫자 필드,
  TXT의 따옴표도 포함해야 합니다. 불일치·NXDOMAIN·빈 응답·시간 초과는 실패입니다.
  `/display`는 응답 문자열, `/count`는 개수입니다. DNSSEC 검증·전체 zone transfer는 아닙니다.
- **TCP**: `kind="tcp"`, `target="example.com:443"` 또는 `[2001:db8::1]:443`입니다.
  `/value`는 연결 시간(ms)이며 도메인을 쓰면 이름 해석 시간도 포함합니다. 연결 후 소켓을
  닫으므로 응용 프로토콜·TLS 통신 검증에는 HTTP나 별도 Lua 검사를 쓰세요.

업데이트한 원격 에이전트에서도 실행할 수 있고 네이티브 네트워크 검사에 스크립트 권한은
필요 없습니다. PostgreSQL·Docker·시스템·장비별 검사는 신뢰하는 외부 스크립트로 확장합니다.
SNMP v2c/v3 GET은 네이티브로 지원하며 숫자 OID와 주입한 인증 정보를 사용합니다. MIB 이름 해석·SET·자동 장비 탐색·trap 수신은 지원하지 않습니다.

## 외부 보고·하트비트

**Push report**(`kind="push"`, `target="push"`) 검사를 저장하고 `interval_seconds`를
허용 보고 간격으로 설정한 뒤 **보고 토큰**을 발급합니다. 토큰은 한 번 보여주고 SHA-256
해시만 저장합니다. 재발급하면 이전 토큰은 폐기되고, 별도로 폐기할 수도 있습니다.
키는 검사 하나에만 연결되며 일시 중지·삭제한 검사는 보고를 거절합니다.

```http
POST /api/push/MONITOR_ID
Authorization: Bearer REPORTING_TOKEN
Content-Type: application/json

{"status":"operational","data":{"value":123,"version":"1.2.3"}}
```

상태는 operational/degraded/outage이고 data는 기록할 JSON입니다. 수치 조건과 주의·장애
규칙도 적용합니다. 공개할 값은 `/value`, `/version` 등으로 선택하고 원본은 비공개입니다.
최대 64 KiB이며 서버 수신 시각으로 기록합니다. Push에서는 응답 시간을 측정하지 않아
latency는 0입니다. 정상 응답은 `{"ok":true,"recorded_at":...}`입니다. 관리자 세션 없이
보고 키로 인증하며 키를 URL이나 Git에 넣지 마세요.

최초 보고에는 초기 대기와 한 보고 간격을 허용합니다. 보고가 없으면 간격마다 실패 관측을
기록하고 연속 실패 조건을 적용합니다. 마지막 실측 값은 원래 시각과 함께 남으며 새 보고로
복구할 수 있습니다. Push는 cron·원격 에이전트·초안 실행을 사용하지 않습니다.
부트스트랩 키와 달리 보고 키는 폐기할 때까지 재사용합니다.

## Cron과 표시 시간대

`cron`을 비우면 기존 주기 검사입니다. `*/5 * * * *`처럼 5개 필드를 쓰고, 6개는 초,
7개는 연도 필드를 추가합니다. `schedule_timezone`은 UTC·Asia/Seoul 같은 IANA 시간대이며
서머타임 규칙을 적용합니다. `initial_delay_seconds`는 검사 생성 후 초기 대기입니다.
검사 하나는 동시에 한 번 실행되고, 중단 중 놓친 스케줄은 한 번으로 합칩니다.
**지금 실행**은 스케줄을 건너뛰지만 동작 후 대기는 지킵니다. 원격 검사는 메인 서버가
실행 시각을 판단하고 기존 fenced lease로 전달합니다.

헤더의 언어·표시 시간대는 드롭다운으로 선택하고 브라우저에 저장합니다. 표시 시간대와
cron 실행 시간대는 별개입니다. 알림 유지보수의 datetime 입력은 브라우저 로컬 시간대로
입력하며 절대 시각으로 저장합니다. API 시각은 UTC Unix 초입니다.

## 검사별 성공·실패 동작

**실패·복구**에서 **실패 시** 또는 **성공·복구 시**를 펼쳐 설정합니다. 모든 기존 검사와
새 검사의 기본값은 동작 없음입니다. Shell/Python/내장 Lua 또는 HTTP(S) JSON POST를
선택합니다. HTTP는 로컬 비공개 엔드포인트를 위해 허용합니다. 원격 관측에 붙인 동작도
**메인 서버**에서 실행하고 메인 서버의 입력을 사용하며 에이전트 인증값을 가져오지 않습니다.
초안 테스트에서는 동작을 실행하거나 큐에 넣지 않습니다.

- 연속 검사 횟수와 검사 자체의 연속 실패·복구 조건 중 큰 값을 충족해야 실행합니다.
- 전환 모드는 그 연속 횟수에 도달할 때 한 번 실행합니다. 최초 성공도 성공 동작을
  실행할 수 있고, 편집하면 연속 횟수가 초기화됩니다.
- 반복 모드는 조건이 유지되는 동안 쿨다운마다 실행하며 최소 쿨다운은 5초입니다.
- 실행 전 지연 중 복구·일시 중지·삭제·설정 revision 변경으로 조건이 사라지면 취소합니다.
- 쿨다운은 검사와 성공/실패 트리거별 새 동작 큐 등록 빈도를 제한합니다.
- 최대 시도는 기본 1, 최대 5회입니다. 실패하면 재시도 간격 후 시도합니다.
  HTTP는 Idempotency-Key, 스크립트는 이벤트 JSON의 delivery_id로 같은 동작을 식별합니다.
- 실행 후 대기는 각 시도 후 다음 검사를 지연해 재시작·초기화를 기다립니다.
  이미 실행 중인 검사는 이 대기로 취소되지 않습니다.
- 각 시도의 타임아웃을 지정하며 시간 초과·종료 시 스크립트 자식도 종료합니다.

큐와 시도 횟수는 SQLite에 저장합니다. 실행 전에 시도를 기록하므로 프로세스가 중단되면
결과가 불확실할 수 있습니다. 기본 1회에서는 불확실한 동작을 재시도하지 않습니다.
재시도를 켜면 중단 이후 중복 실행될 수 있으니 멱등하게 작성해야 합니다. 실행 중인 동작은
편집으로 되돌릴 수 없습니다. 공통 알림 유지보수는 복구 동작을 중지하지 않습니다.
동작을 끄거나 검사를 일시 중지하세요. 전달 로그에 실제 시도 횟수와 결과를 표시하며
동작 출력은 공개 모니터링 값에 넣지 않습니다.

스크립트에 `SACHO_EVENT_JSON`을 주입하며 event/item_id/name/status/previous_status/at/
latency_ms/선택 공개값 display_value/attempt/delivery_id가 들어갑니다.
Lua는 `sacho.json_decode(sacho.env("SACHO_EVENT_JSON"))`으로 읽습니다. JSON 템플릿도
같은 문맥을 사용합니다.

```jinja
{"event":{{ event|tojson }},"monitor":{{ name|tojson }},"status":{{ status|tojson }}}
```

원본 검사 출력이나 인증값은 문맥에 들어가지 않습니다. JSON 템플릿은 엄격한 변수 검사와
크기 제한을 적용하고 인증 헤더는 동작별 headers_env에서 읽습니다. 검사별 동작과 기존
공통 구독 메일·웹훅은 함께 동작하며 공통 알림 지연·쿨다운·유지보수 정책은 그대로 적용됩니다.

## LLM 설정 자동화용 정확한 API 형식

`/api/admin/*`는 설정된 관리자 인증이 필요합니다. 변경 요청에는
`X-Sacho-Request: 1`을 보내고, 브라우저 Origin은 기본 공개 주소나 `public_origins`와
일치해야 합니다. 기존 인증 프록시 세션을 사용하세요. 공개 프록시에 임의 사용자 헤더를
보낸다고 관리자 권한이 생기는 것으로 가정하면 안 됩니다.

| 경로 | 메서드 | 요청·응답 |
| --- | --- | --- |
| `/api/admin/examples` | GET | id/title/spec/inputs/requirements 예제 목록 |
| `/api/admin/validate` | POST | 검사 spec → valid/errors/warnings/required_env |
| `/api/admin/test` | POST | 로컬 검사 spec → 저장·동작 실행 없는 관측 결과 |
| `/api/admin/items` | POST | 검사 spec → 생성된 id |
| `/api/admin/items/{id}` | PUT | 검사 spec 교체, 연속 횟수 초기화·이전 revision 동작 취소 |
| `/api/admin/inputs` | GET | vault_configured/scope/암호화 값 제외·일반값 조회 가능한 inputs 메타데이터 |
| `/api/admin/inputs/{name}` | PUT | `{"value":"새 비밀값","enabled":true,"encrypted":true}`; value를 생략하면 주입 허용만 변경 |
| `/api/admin/inputs/{name}` | DELETE | vault 값·동적 허용 삭제, 서버 설정은 유지 |
| `/api/admin/push/{id}/token` | POST | token/endpoint/shown_once, 기존 키 교체 |
| `/api/admin/push/{id}/token` | DELETE | 보고 키 폐기 |

예제 spec을 읽고 필요한 필드만 바꾼 뒤 검증·저장합니다. 비밀값은 보호된 inputs 경로로
전달하고 target·동작 body·헤더 리터럴에는 넣지 마세요. 조회 API로 저장된 비밀값을 복구할
수는 없습니다. 검사 `actions` 배열은 트리거별 하나씩 최대 두 개입니다.

```json
{
  "trigger":"failure","mode":"transition","kind":"https",
  "target":"https://example.com/monitor-hook",
  "body":"{\"event\":{{ event|tojson }},\"status\":{{ status|tojson }}}",
  "headers_env":{"Authorization":"HOOK_AUTHORIZATION"},
  "after_count":3,"delay_seconds":10,"cooldown_seconds":300,
  "max_attempts":1,"retry_seconds":30,"wait_after_seconds":30,"timeout_seconds":15
}
```

trigger는 success/failure, mode는 transition/repeat, kind는 shell/python/lua/https입니다.
https 종류는 HTTP(S) POST를 뜻하며 스크립트 종류의 target에는 소스를 넣습니다.
횟수 1..100, 시도 1..5, 타임아웃 1..300초, 시간 필드 0..86400초이고 재시도 간격은
1..86400초입니다. 반복 쿨다운은 최소 5초입니다. 동작 결과로 검사 관측 상태를 바꾸지
않습니다. 실서버 동작을 켜기 전에 격리한 환경에서 실행 대상을 검증하세요.

## 종류별 활성화·접기·그래프 (v0.1.10)

PromQL은 Prometheus, DNS 옵션은 DNS, HTTP 상태·헤더는 HTTP/Prometheus,
종료 코드는 스크립트에서만 보이고 활성화됩니다. Push에서는 로컬 실행·cron 설정을
숨깁니다. 값·추세 선택에 따라 관련 입력을 활성화하며, 비활성 HTTP 헤더의 잘못된 JSON이
Lua 저장을 막지 않습니다. 수치 설정을 접거나 펼칠 수 있고 수치 바로가기는 접힌 설정을
엽니다. 이미 규칙·추세가 설정되어 있으면 자동으로 펼칩니다. 공개 그래프도 개별로 접고,
수치/응답 시간과 곡선/직선을 선택할 수 있으며 자동 갱신 중 선택을 유지합니다.
곡선은 실제 버킷 값 사이의 형태를 보존하고 과도하게 튀지 않도록 보간합니다. 빈 구간은
이어 그리지 않으며 호버 값은 실제 평균·최소·최댓값입니다.

## 일반값·암호화 값 선택과 항목별 바인딩

전역 환경변수 관리에 **비밀값 암호화** 선택이 있습니다. PUT 요청의 encrypted는 기본 true로
기존 요청과 호환됩니다. false이면 일반값을 SQLite에 평문 저장하고 관리자에서 조회·편집할
수 있습니다. true이면 vault에 암호화하고 반환하지 않습니다. 모드 변경에는 새 값을 입력해야
하며 체크 해제로 기존 비밀값을 노출하지 않습니다. 일반값은 vault 설정 없이도 사용할 수 있습니다.

검사 편집의 입력 영역에서 이름·값·암호화 여부를 선택해 추가·삭제합니다. 해당 검사와 그 메인
서버 동작에만 주입하고 같은 이름의 전역 입력보다 우선합니다. 다른 검사에는 허용하지 않습니다.
암호화 값은 편집 창에서 비어 있으며 비워두면 유지, 새 값을 입력하면 교체, 행을 제거하면
삭제합니다. 교체는 새 vault 항목을 먼저 준비하고 검사 spec을 원자적으로 변경한 뒤 이전 항목을
정리합니다. 저장 실패 시 새 항목을 정리하고 검사 삭제 시 소유 비밀값도 삭제합니다. 중단된
저장·정리 작업은 사용하지 않는 암호화 항목을 남길 수 있습니다. SACHO_ITEM_ 이름은 내부용이며
전역 입력 목록에 노출하지 않습니다. 안전한 내보내기는 바인딩 값·참조를 제외합니다.

검사 저장·로컬 초안 테스트에 다음 임시 input_updates를 보낼 수 있습니다.

```json
"input_updates": [
  {"name":"CHECK_URL","value":"https://example.com/metrics","encrypted":false},
  {"name":"AUTHORIZATION","value":"Bearer YOUR_PRIVATE_TOKEN","encrypted":true}
]
```

input_updates는 저장·반환하지 않습니다. 저장된 input_bindings는 일반값 또는 암호화 내부
참조입니다. 편집 시 유지하고 키를 제거하면 바인딩을 삭제하며 값을 교체할 때는 input_updates를
사용합니다. 암호화 참조를 다른 검사로 복사할 수 없습니다. 초안 테스트는 메모리에서만 처리하고
vault·DB에 저장하지 않습니다. 스크립트가 입력값을 출력할 수 있으므로 인증값을 출력하거나
공개값으로 선택하지 마세요.

원격 검사의 일반값은 인증된 에이전트 연결로 전달합니다. 메인 서버에서 암호화한 바인딩은
원격에 전달하지 않습니다. 에이전트 자체 허용 목록·vault에 비밀값을 설정하고 변수 이름으로
참조하세요. 화면에도 이 차이를 안내하고 원격 암호화 입력을 비활성화합니다. 메인 서버 동작은
메인 서버에 소유한 입력을 사용합니다.

## 네이티브 ICMP·SNMP

ping은 외부 ping이나 Lua 없이 Rust에서 ICMP echo를 실행합니다. 호스트 이름·IPv4·IPv6를
대상으로 사용하고 ping 설정에 count(1..10, 기본 3), min_replies(1..count, 기본 1),
wait_ms(100..10000, 기본 1000), interval_ms(100..5000, 기본 100)를 지정합니다. 최대 대기
합계가 검사 타임아웃 안에 들어야 합니다. 최소 응답보다 적으면 실패합니다. healthy/rtt_ms/
rtt_min_ms/rtt_max_ms/loss_percent/sent/received/message를 반환합니다. RTT는 실제 ICMP
응답 시간이며 응답이 없으면 0을 만들지 않고 null입니다. /rtt_ms로 값·추세를, /loss_percent로
손실률 규칙을 설정할 수 있습니다. 도메인은 매번 해석한 주소 하나를 사용하므로 두 IP 계열을
검증하려면 검사를 분리하세요. 실행 노드의 ping 소켓 권한 또는 raw socket 권한이 필요하며
프로그램이 시스템 권한을 자동 변경하지 않습니다. ICMP 차단과 HTTP 장애는 별개입니다.

SNMP는 host:161 또는 [IPv6]:161을 조회하고 snmp 객체에 다음 설정을 넣습니다.

```json
{
  "version":"3","oid":"1.3.6.1.2.1.1.3.0","username":"monitor-user",
  "security":"authPriv","auth_protocol":"sha256",
  "auth_env":"SNMP_AUTH_PASSWORD","privacy_env":"SNMP_PRIVACY_PASSWORD"
}
```

v2c는 community_env(기본 SNMP_COMMUNITY)를 사용합니다. v3는 noAuthNoPriv/authNoPriv/
authPriv, SHA1/SHA256 인증·AES128 암호화를 지원합니다. 비밀번호는 변수 이름으로 참조하며
엔진 발견·시각 동기화를 네이티브로 수행합니다. value/type/oid를 반환하고 /value를 수치·규칙·
추세에 연결합니다. 정수·counter32/64·gauge32·timeticks(1/100초), 문자열·IP·OID 스칼라를
지원합니다. OID 부재·인증 실패·지원하지 않는 구조값·시간 초과는 실패입니다. 읽기 전용 GET이며
SET·외부 snmpget·MIB 이름 DB·trap 데몬·자동 장비 탐색은 포함하지 않습니다. v2c의 community는
프로토콜상 암호화하지 않으므로 필요하면 v3 authPriv를 사용하세요. 최신 에이전트에서도
스크립트 권한 없이 네이티브 검사를 실행합니다.

검사 목록은 **검사 템플릿**으로 표시합니다. API는 /api/admin/check-templates를 권장하고
기존 /api/admin/examples도 유지합니다. CLI는 sacho template이며 example도 호환합니다.
ping/snmp는 네이티브 JSON 템플릿입니다. shell-disk(df/awk), shell-process(ps/awk),
shell-http(curl/sed)는 디스크·정확한 프로세스 이름·인증된 사용자 정의 HTTP 예제입니다.
curl 인증 헤더는 인자 대신 표준입력으로 전달합니다. 일반 HTTP/JSON은 네이티브 템플릿을,
사용자 정의 처리에는 명시적으로 구분한 Lua 어댑터를 선택하세요. 템플릿은 초안만 엽니다.

SNMPv3 인증·암호화 비밀번호는 최소 8바이트이며 짧거나 빈 값은 키 생성 전에 안전하게 실패합니다.
