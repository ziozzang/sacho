# Lua 시나리오

[English](LUA.md)

관리자에 hangang의 CodeMirror 에디터를 적용했습니다. 구문 강조·행 번호·
실행 API 자동완성·괄호·실행 취소를 지원합니다. Ctrl+Space로 자동완성을
열고 **시나리오 테스트**로 저장 없이 서버에서 실행합니다. 언어를 바꿔도
초안이 유지됩니다. 에디터는 실행 파일에 포함되며 CDN을 사용하지 않습니다.

검사마다 `sacho probe-lua` 하위 프로세스를 새로 실행합니다. Lua 메모리
16 MiB, 코드 64 KiB, 설정한 시간 제한을 적용하고 Unix에서 시간 초과·취소 시
프로세스 그룹을 종료합니다. HTTP 본문과 저장 로그도 64 KiB로 제한합니다.

Lua 기본·table·string·math·utf8를 사용할 수 있고 `os`, `io`, `package`,
`debug`, 파일 접근은 제외합니다. 신뢰된 관리자가 운영자 네트워크에서
실행할 시나리오를 작성하는 방식입니다.

| API | 기능 |
| --- | --- |
| `sacho.api_version` | 숫자 1 |
| `sacho.http(url)` | GET 후 status·body·latency_ms 반환 |
| `sacho.http({url, method, headers, body})` | 요청 옵션 지정. TLS 검증, 리다이렉트 미추적 |
| `sacho.json_decode(text)` | JSON을 Lua 값으로 변환 |
| `sacho.json_encode(value)` | Lua 값을 JSON 문자열로 변환 |
| `sacho.env(name)` | `probe_env`에 명시한 환경변수만 조회 |
| `sacho.exec({program, arg1, ...})` | 쉘 확장 없이 외부 명령 실행. `{exit_code, stdout, stderr}` 반환. 스트림별 64 KiB 캡처, 검사 시간 제한과 프로세스 그룹 공유. |

true·숫자·문자열·nil 반환은 성공, false·assert 실패·런타임 오류는 실패입니다.
테이블은 JSON으로 변환해 반환하세요. HTTP 오류 코드는 직접 assert로
검증합니다. 연결·TLS·시간 초과 오류는 예외로 처리됩니다.

```lua
local response = sacho.http("https://gateway.example.com/health/liveliness")
assert(response.status == 200, "Gateway routing failed")
assert(sacho.json_decode(response.body).status == "alive", "Dorang is not live")
return sacho.json_encode({value = "gateway → dorang"})
```

표시 JSON 포인터 `/value`와 최신값 공개 표시를 사용하세요.
`{"tps":42.5}`는 `/tps`와 단위 ` TPS`, `{"version":"v1.2.3"}`는
`/version`으로 표시합니다. 표시값과 숫자 조건은 독립적이며 숫자 임계값을
비워두면 조회값 자체로 장애를 판정하지 않습니다. 출력 문자열은 160자로
제한하고 화면에서 이스케이프합니다. 객체·배열은 직접 공개하지 않습니다.

인증 헤더는 `sacho.env`에서 읽고 로그에 출력하지 마세요.
서비스별 API와 인증 파일 처리는 외부 시나리오에 두며 본체는 이를 알지 못합니다.
`sacho.exec`는 쉘·Python 검사와 동일하게 신뢰된 관리자가 서버 사용자 권한으로
외부 프로그램을 호출합니다. 허용한 환경변수와 고정 PATH/LANG만 전달하며
표준입력은 닫습니다. JSON을 읽기 전에 종료 코드가 0인지 검사하세요.
없는 조회값을 정상이나 잔여량 100%로 추정하지 마세요.

[실제 예제](../examples/lua)를 환경에 맞게 수정해서 사용하세요.

### 실패 상태에서의 조회값

새 검사가 조회값 없이 실패해도 마지막 값은 사라지지 않습니다.
`display_value_at`과 `display_value_stale`로 시각과 과거 값 여부를 제공하고,
화면에 **이전 조회값**과 시각을 표시합니다. 값이 한 번도 없으면 **수치 조회 불가**로
표시하며 0으로 만들지 않습니다. 실패한 검사에서도 반환된 수치는 표시됩니다.
`public_message_pointer="/message"`를 지정하면 최신 JSON 출력에서 선택한 짧은
문자열만 공개합니다. 내부 오류·로그는 공개하지 않습니다. 실패 원인을 공개하려면
assert로 끝내지 말고 JSON을 반환하고 숫자 조건으로 판정하세요. 공개 필드에는
비밀값을 포함하지 마세요.

### Docker 컨테이너와 Unix 소켓

`sacho example docker`로 내장 예제를 출력할 수 있습니다.
`sacho.http({url="http://localhost/...", unix_socket="/절대경로/docker.sock"})`는
프록시와 리다이렉트를 사용하지 않고 본문 없는 GET만 허용합니다.
실행 노드의 `probe_env`에 `DOCKER_SOCKET`, `DOCKER_CONTAINER`를 허용하고,
조건은 `/healthy gte 1`, 조회값은 `/display`, 공개 설명은 `/message`로 설정합니다.
HEALTHCHECK가 없는 실행 중 컨테이너는 정상이고, starting·unhealthy·중지·누락은
실패합니다. 별도 Docker CLI나 Lua 라이브러리는 필요 없습니다. Rootless 소켓이나
Docker 호스트의 원격 워커에서도 실행할 수 있습니다.

Docker 소켓은 강력한 호스트 권한을 주므로 신뢰한 검사에만 허용하세요.
읽기 전용 바인드 마운트는 Docker API 변경을 막지 않습니다. 내장 소켓 HTTP는 GET으로
제한하지만 스크립트 실행 자체가 보안 샌드박스는 아닙니다. 강한 격리가 필요하면
제한된 소켓 프록시를 사용하세요. 응답은 64 KiB까지 허용하며 큰 응답은 실패합니다.
컨테이너 환경변수 원문은 공개하지 않고 선택한 수치와 설명만 공개합니다.

잔량·정보 조회 검사에서 `failure_status="degraded"`를 지정하면 조건 실패,
접속 오류, 시간 초과를 주의로 처리합니다. 기본값은 `outage`입니다.
실패 기록과 설명은 그대로 표시되며 모니터링 알림도 유지합니다.
