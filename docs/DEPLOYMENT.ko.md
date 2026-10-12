# 배포

[English](DEPLOYMENT.md)

Linux/macOS용 실행 파일 하나로 실행합니다. 원자적 업데이트는 Unix에서
지원합니다. GNU Linux 빌드는 호환되는 glibc가 필요하고 musl 빌드는 정적
실행 파일입니다. SQLite·Lua·웹 자산은 모두 내장합니다.

로컬 평가는 `sacho init` → `sacho serve`로 시작합니다.
외부 공개 시 no-auth를 프록시로 노출하지 말고 다음과 같이 설정하세요.

```toml
bind = "127.0.0.1:8080"
database = "/home/sacho/data/sacho.db"
public_url = "https://sacho.example.com"
[auth]
mode = "auth-proxy"
email_header = "X-Forwarded-Email"
admins = ["operator@example.com"]
```

프록시와 같은 호스트 네트워크이면 루프백을 사용합니다. 그 외에는
방화벽으로 제한한 사설 인터페이스를 사용하세요. 프록시는 외부 신원 헤더를
제거하고 검증된 이메일을 주입해야 합니다. Sacho는 그 헤더를 신뢰하고
등록된 관리자 이메일과 정확히 비교합니다.

[hangang 설정 예제](../deploy/hangang.example.json)는 `/`를 공개하고
`/_hopae`, `/admin`, `/api/admin/`을 더 높은 우선순위의 인증 경로로 처리합니다.
`/_hopae` 경로는 Google 로그인 후 Sacho 도메인에 세션 쿠키를 심는
핸드오프를 처리합니다. 이 경로가 없으면 Google 로그인은 성공해도 Sacho는
로그인 상태가 되지 않습니다. 공개 경로를 포함해 Hopae 쿠키를 업스트림에서
제거합니다. 별도의 HTTP 경로는 업스트림 호출 전에 HTTPS로 이동합니다.
호스트·리스너 ID·백엔드·인증 계약을 현재 hangang 설정에 맞게 바꾸세요.
Hopae에는 Sacho 호스트를 앱으로 등록합니다. hangang의 `HANGANG_HOPAE_KEY`
환경변수는 클라이언트가 보낼 수 없어야 하며 Sacho에 복사할 필요도 없습니다.

공개 API는 검사 코드·대상 URL·원본 출력·구독 이메일·웹훅 주소를 반환하지
않습니다. 공개를 선택한 최신 조회값만 표시합니다. `/admin`의 HTML을 공개해도
관리자 API는 여전히 인증을 요구하지만 프록시에서 두 경로 모두 보호하는
구성을 권장합니다.

[systemd 예제](../deploy/sacho.service)의 사용자·경로를 바꾸고 전용 계정으로
실행하세요. 비밀번호는 비공개 EnvironmentFile에 두고, 자동 업데이트를
사용할 때는 실행 파일 디렉터리를 서비스 사용자가 쓸 수 있게 합니다.
서비스 명령은 `serve`를 사용합니다. 내부 `--worker`는 직접 실행하지 않습니다.

인증·SMTP·업데이트 설정은 TOML 변경 후 재시작합니다. 사이트 제목·설명,
검사 항목, 알림 수신 대상, 레이아웃은 관리자 화면에서 변경합니다.
SMTP 비밀번호와 검사 비밀값은 관리자 화면에서 표시·변경하지 않습니다.

SMTP는 `starttls` 또는 `tls`를 사용합니다. 테스트용 `plain`은 루프백으로
제한합니다. `public_url`을 실제 주소로 지정해야 구독 메일 링크와 Origin
검증이 일치합니다. 확인 링크는 24시간 유효하고, 확인 메일은 전체 시간당
100건·주소별 시간당 1건으로 제한합니다.

검사 기본 보존 기간은 100일이며 90~3650일로 설정합니다. 검사·완료/소진된
알림·미확인 구독은 시간마다 정리합니다. 장애 이력·확인된 구독은 유지합니다.
DB 백업은 SQLite 백업 API를 사용하거나 서비스를 중지한 뒤 복사하세요.
실행 중인 WAL DB의 본체만 복사하지 마세요. SIGTERM 종료 시 WAL을 정리합니다.

자동 업데이트는 시작 시와 설정한 주기마다 서명을 검증합니다. 버전·타깃·URL·
SHA-256·크기를 고정하고 버전 하향을 차단합니다. 사설 HTTPS 배포 서버는
`manifest_url`과 선택적 `ca_file`을 사용할 수 있습니다.

교체 전 새 바이너리의 버전을 실행해 확인한 뒤 워커를 중지·교체·재시작합니다.
준비 확인 실패 또는 30초 관찰 중 종료 시 이전 바이너리를 복구합니다.
정상 관찰 완료 후 백업을 삭제합니다. 감독 프로세스 자체의 코드는 서비스
재시작까지 기존 버전이며 후속 업데이트 비교에는 새 워커 버전을 사용합니다.
관찰 중 전원 장애가 나면 복구용 파일이 남을 수 있습니다. 자동 복구는 실행 중인
감독 프로세스가 담당하며 별도의 부팅 복구 저널을 제공하지는 않습니다.

릴리스 서명용 32바이트 Ed25519 시드는 저장소 밖과 GitHub Secret
`SACHO_RELEASE_SIGNING_KEY`에 보관합니다. 공개키만 저장소에 커밋합니다.
Cargo 버전과 일치하는 `v<버전>` 태그를 푸시하면 Linux/macOS x86_64/ARM64와
Linux x86_64 musl 실행 파일을 빌드하고 서명된 매니페스트를 게시합니다.

`/api/account` must use the same identity-stripping authentication proxy as the protected admin API. It reports only the authenticated email and its subscription state. Set `[auth].logout_url` to your proxy logout path (for Hopae, `/_hopae/logout`) to enable the logout link. A missing setting hides the link. Plain SMTP is supported only for loopback or private IPv4 relays.
