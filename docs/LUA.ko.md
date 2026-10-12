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
