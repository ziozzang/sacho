# Sacho · 사초

v0.1.4에는 검증 가능한 SQLite/PostgreSQL 스냅샷 이전과 PostgreSQL 작업 임대·펜싱
기반과 원격 모니터링 워커를 추가했습니다. 중앙 저장소는 SQLite이며 다중 중앙 HA는
미룹니다. [원격 워커](docs/REMOTE-WORKERS.ko.md)는 같은 실행 파일로 운영합니다.
[저장소 안내](docs/DATABASE.ko.md)(`sacho help storage --language ko`)와
[기업용 기능 구현 현황](docs/architecture/ENTERPRISE.md)을 참고하세요.

공개 상태 페이지와 지속적인 헬스 체크를 제공하는 단일 실행 파일입니다.
사초는 실록의 바탕이 되는 기록입니다. 서비스의 현재 상태와 장애·복구 이력을
명확하게 기록합니다.

제품 범위는 모니터링·상태와 이력 표시·알림 전달입니다. 장애 관리, 담당자 배정,
확인·온콜·대응 업무는 웹훅으로 연결한 외부 시스템에서 처리합니다.
기존 `incident.started`/`incident.resolved`와 `incident_id`는 자동 감지한
장애·복구의 연결 정보이며 호환성을 위해 유지합니다.

[English](README.md) · [Lua 시나리오](docs/LUA.ko.md) · [배포](docs/DEPLOYMENT.ko.md)

HTTP·Shell/Python/Lua 외에 네이티브 Prometheus 순간 쿼리, DNS 레코드, TCP 연결 시간,
검사별 인증된 Push 보고를 지원합니다. 관리자에서 암호화 입력을 교체하면 다음 검사부터
반영됩니다. IANA 시간대의 cron과 선택적 메인 서버 동작에 연속 횟수·지연·재시도·쿨다운·
실행 후 대기를 지정할 수 있습니다. 언어·표시 시간대는 드롭다운으로 선택해 저장합니다.
정확한 지원 범위와 동작은 [내장 모니터링 안내](docs/MONITORING.ko.md)를 참고하세요.

## 실행

[릴리스](https://github.com/ziozzang/sacho/releases)에서 OS·아키텍처에 맞는
실행 파일을 받아 `sacho`로 저장하고 실행 권한을 부여하세요. 소스 빌드도 가능합니다.

```sh
cargo build --locked --release
./target/release/sacho init --config sacho.toml
./target/release/sacho doctor --config sacho.toml
./target/release/sacho serve --config sacho.toml
```

기본 주소는 `http://127.0.0.1:8080/`, 관리자는 `/admin`입니다.
기본 no-auth 모드는 루프백에서만 실행할 수 있습니다. 외부 공개 시에는
인증 프록시와 관리자 이메일을 설정하세요.

실행 파일에 웹 화면·CodeMirror Lua 에디터·Lua 5.4·SQLite를 포함합니다.
실행 시 Node.js·CDN·외부 DB·Lua 설치가 필요하지 않습니다. Python 검사는
선택적으로 `python3`, 쉘 검사는 `/bin/sh`를 사용합니다. 설정 파일과 SQLite
데이터 파일은 실행 파일과 별도로 보관합니다.

수치 임계값·추이·워커 상태·설정 검증·알림 억제는
[모니터링 설정](docs/MONITORING.ko.md)과 내장 `sacho help monitoring --language ko`를
참조하세요.

## 기능

- 한국어·영어 반응형 상태 화면, 서비스별 가용성, 장애·복구 이력.
- HTTP/API, 쉘, Python, 내장 Lua 검사. 여러 HTTP 응답을 검증하는 Lua 시나리오와
  구문 강조·자동완성·저장 전 테스트를 제공합니다.
- 검사 주기·시간 제한·HTTP/종료 코드·문자열 포함·JSON 숫자 임계값·응답 지연·
  연속 실패/복구 횟수 조건을 설정합니다.
- 최신 조회값을 상태와 별도로 표시합니다. 버전 문자열·TPS·사용량 잔여 비율 등
  숫자·문자열을 공개할 수 있습니다. 공개 여부는 항목별로 선택하며 원본 출력과
  검사 코드·인증 정보는 관리자만 확인합니다.
- 5초·10초·30초·1분·5분·30분·1시간·1일 단위 이력 조회와 90일 조회.
  회색 구간은 데이터 없음입니다. 가동률은 **관측된 검사 결과** 기준이며
  관측하지 않은 시간의 정상 운영을 추정하지 않습니다.
- 조회 기간 변경 시 검사 주기에 맞는 타임갭을 자동 선택합니다. 1시간
  조회에 1일 단위를 적용하는 것처럼 맞지 않는 옵션은 제외합니다.
- 섹션 이름·서비스 순서를 드래그 앤 드롭이나 이동 버튼으로 편집하고
  미리보기 후 저장하는 레이아웃 에디터.
- 장애 시작·복구 시 이메일/웹훅 알림. 상태·장애·전송 작업을 함께 저장하고
  반복 실패에 대한 장애 알림 작업을 중복 생성하지 않습니다.
- 서명된 HTTPS/GitHub 업데이트, 해시·크기·타깃 검증, 원자적 실행 파일 교체,
  자동 재시작과 초기 실행 실패 시 이전 실행 파일 복구.

## 실제 시나리오

관리자에서 검사 종류를 Lua로 선택하고 스크립트를 입력하세요.
**시나리오 테스트**는 저장·장애 생성·알림 전송 없이 초안을 한 번 실행합니다.
저장하면 주기적으로 실행하고 **지금 검사**와 **로그**로 결과를 확인합니다.

```lua
local live = sacho.http("http://127.0.0.1:4000/health/liveliness")
assert(live.status == 200, "Dorang liveness failed")
local ready = sacho.http("http://127.0.0.1:4000/health/readiness")
assert(ready.status == 200, "Dorang readiness failed")
return sacho.json_encode({value = "live + ready"})
```

최신 조회값 공개 표시를 켜고 표시 JSON 포인터를 `/value`로 설정하세요.
버전 결과 `{"version":"v1.2.3"}`는 `/version`, TPS 결과는 `/tps`와
표시 단위 ` TPS`를 사용합니다. 표시만 하려면 숫자 임계값을 비워둡니다.
숫자 조건 JSON 포인터와 표시 JSON 포인터는 각각 설정할 수 있습니다.

[`examples/lua`](examples/lua)에 hangang → dorang 검사, dorang 생존·준비
검사, TPS, 구독 잔여량 예제가 있습니다. 서버 주소는 환경에 맞게 바꾸세요.
[`tools/import_checks.py`](tools/import_checks.py)로 예제를 등록할 수도 있습니다.

Sacho 본체에는 특정 서비스의 사용량 API나 로그인 파일을 읽는 코드가 없습니다.
서비스별 로직은 외부 Lua/Python/쉘 시나리오에 두세요. Lua에서는
`sacho.exec({"python3", "/opt/checks/health.py"})`로 외부 프로그램을 실행하고
종료 코드와 JSON 표준출력을 검사할 수 있습니다. 환경변수 허용 목록,
출력 크기 제한, 검사 시간 제한과 프로세스 그룹 정리가 동일하게 적용됩니다.

## 알림·인증·업데이트

SMTP 비밀번호는 환경변수로만 전달합니다. SMTP 미설정 시 이메일 구독은
화면에서 비활성화됩니다. 구독 확인 링크는 24시간 유효하고 확인·해지 작업은
POST로만 처리하므로 메일 스캐너의 GET 요청이 구독 상태를 바꾸지 않습니다.
웹훅은 관리자 화면에서 등록·삭제하고 전송 결과를 확인합니다.

전송 실패는 최대 8회 재시도합니다. 웹훅의 `delivery_id`와 `Idempotency-Key`는
재시도 중에도 같으므로 수신 측에서 중복을 제거할 수 있습니다. SMTP 전송 후
응답이 유실되는 경우까지 정확히 한 번의 메일 전송을 보장하지는 않습니다.

외부 설정 파일의 인증 모드는 `no-auth` 또는 `auth-proxy`입니다.
프록시는 클라이언트가 보낸 신원 헤더를 제거하고 검증된 이메일을 주입해야 합니다.
관리자 API 변경 요청은 `X-Sacho-Request: 1`과 올바른 Origin을 요구합니다.

```sh
sacho update --check
sacho update --config sacho.toml # 서버를 중지한 상태에서 수동 설치
```

`[updates].enabled = true`이면 시작 시와 설정한 주기마다 서명된 릴리스를
확인합니다. `serve`는 같은 실행 파일로 감독 프로세스와 워커를 실행합니다.
업데이트 후 준비 확인에 실패하거나 30초 관찰 중 종료되면 이전 바이너리로
복구합니다. 정상 실행을 확인한 뒤 백업 바이너리를 삭제합니다. 실행 파일
디렉터리는 서비스 사용자가 쓰기 가능해야 합니다. 수동 설치는 백업을 남기고
재시작이 필요합니다. 실행 잠금으로 같은 DB의 중복 서버·수동 업데이트를 막습니다.

릴리스 서명 비밀키는 저장소 밖과 GitHub Secret에 보관합니다. 서버에는
공개키만 필요합니다. 향후 DB 스키마 변경 전에는 DB를 백업하세요.
기존보다 오래된 바이너리가 새 DB 스키마를 여는 작업은 차단합니다.

관리자는 신뢰된 운영자입니다. 쉘·Python 검사는 서비스 사용자 권한으로 서버에서
실행되므로 전용 계정 사용이 적절합니다. Lua는 파일·OS·패키지·디버그 API를
제외하고 16 MiB 메모리·실행 시간 제한과 명시적 호스트 API를 적용합니다.

기본적으로 빌드·검증은 로컬에서 수행합니다. 비용 관리를 위해 GitHub Actions는
명시적인 수동 실행만 지원하며 push·태그·PR로 자동 실행되지 않습니다.
빌드·통합·브라우저·업데이트 테스트 명령은 [영문 문서](README.md)에 있습니다.
MIT 라이선스. [참고 코드·라이선스](THIRD_PARTY.md).

## LLM 설정 가이드

[영문 가이드](docs/SETUP-HOWTO.md) · [한국어 가이드](docs/SETUP-HOWTO.ko.md)

```sh
sacho help setup
sacho help setup --language ko
sacho example prometheus
sacho example postgresql
sacho secrets --help
```

관리자 → 알림에서 Jinja 형식의 메일 제목·본문과 웹훅 JSON 템플릿을 편집하고
전송 없이 미리볼 수 있습니다. 사용 변수와 API는 설정 가이드에 정리했습니다.
인증값은 템플릿 context에 넣지 않습니다.

원격 에이전트는 관리자 에이전트 탭에서 일회용 부트스트랩 키로 등록하고 검사별
선택 목록에서 지정할 수 있습니다. `sacho worker-join --config worker.toml`은
키를 표준입력으로 받아 자체 인증정보를 암호화 저장하고 중앙 정보와 교환합니다.
중앙은 해시만 보관하며 등록 응답 손실 복구와 즉시 권한 철회를 지원합니다.

Docker 컨테이너는 `sacho example docker`의 내장 Lua 예제로 Unix 소켓을 통해 조회할
수 있습니다. 별도 Docker CLI 없이 실행·중지·헬스체크 상태를 검사합니다.
조회 실패 시에도 마지막 수치와 시각이 남으며, 공개 설명과 주의 등급을 설정할 수
있습니다. 자세한 설정은 [Lua 문서](docs/LUA.ko.md)를 참조하세요.
