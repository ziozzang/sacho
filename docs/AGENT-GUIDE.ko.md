# 에이전트가 혼자 Sacho 설치하기

[English](AGENT-GUIDE.md) · [전체 배포 안내서](DEPLOY.ko.md)

자동화 에이전트나 설치를 스크립트로 만드는 사람을 위한 절차서입니다. 단계마다 명령과 통과 조건이
있고, 웹 브라우저가 필요 없습니다. 이유는 `DEPLOY.ko.md`에, 더 자세한 API는 인터넷 없이
`sacho help setup --language ko`에 있습니다.

## 규칙

- **고치기 전에 먼저 봅니다.** 이미 설치되어 있으면 버전, 서비스 단위, 설정, `GET /api/admin/items`를
  읽습니다. 잘 돌고 있는 배포를 통째로 바꾸지 않습니다. 모니터는 ID로 갱신해 이력을 지킵니다. 지우고
  다시 만들지 않습니다.
- **비밀값을 드러내지 않습니다.** 표준 입력이나 비공개 파일로 넘기고, 인자·로그·TOML·점검 스크립트·
  보고서에 넣지 않습니다. 보고에는 값이 아니라 이름을 적습니다.
- **운영자만 아는 값을 짐작하지 않습니다.** 아래 표에서 "묻기"인 값은 멈추고 물어봅니다.
- **Sacho 포트를 절대 열지 않습니다.** 닿기만 하면 누구든 어떤 신원이든 주장할 수 있습니다.
- **알림을 시험하려고 실제 장애를 일으키지 않고**, 시험 메일을 실제 사람에게 보내지 않습니다.

## 1단계: 값 정하기

| 값 | 예 | 출처 | 비고 |
| --- | --- | --- | --- |
| 설치 방식 | 시스템 서비스 | 선택 | root가 있으면 시스템 서비스, 없으면 사용자 서비스. |
| 공개 호스트 이름 | `status.example.com` | 묻기 | 프록시를 가리켜야 합니다. `public_url`이 됩니다. |
| 추가 호스트 이름 | 없음 | 묻기 | `public_origins`. |
| 관리자 이메일 | `admin@example.com` | 묻기 | `[auth].admins`에 넣을 정확한 주소. |
| 역방향 프록시 | nginx | 탐지, 없으면 묻기 | nginx, Caddy, Traefik, 또는 forward-auth 되는 다른 것. |
| 인증 서비스 | oauth2-proxy | 탐지, 없으면 묻기 | 확인 주소와 돌려주는 헤더가 필요합니다. |
| 신원 헤더 | `X-Forwarded-Email` | 도출 | 기본값. Traefik만 확인기의 헤더를 그대로 씁니다. |
| 내부 주소 | `127.0.0.1:8080` | 빈 포트 선택 | 공개되면 안 됩니다. |
| 메일 중계 서버 | 없음 | 묻기 | 호스트, 포트, 보내는 사람, 보안 방식, 비밀번호 출처. 나중에 추가 가능. |
| 자동 업데이트 | 끔 | 묻기 | 사람 없이 서비스를 재시작합니다. 허락이 없으면 끕니다. |
| 모니터 | HTTP 대상 | 묻기 | 대상, 주기, 무엇을 `public`으로 둘지. |
| 비밀값 출처 | 비밀번호 관리 도구 | 묻기 | 비밀번호를 어디서 가져올지. |

프록시나 인증 서비스가 없으면 물어봅니다. 시험용 확인기를 닿을 수 있는 주소에 대신 띄우면 안 됩니다.
답이 없으면 loopback의 `no-auth`까지만 끝내고 남은 일을 보고하면 됩니다.

## 2단계: 설치

```sh
VERSION=v0.2.1
TARGET=x86_64-unknown-linux-musl
BASE=https://github.com/ziozzang/sacho/releases/download/$VERSION
ASSETS=$BASE/sacho-$VERSION-deploy-docs.zip
curl -fsSLO "$BASE/sacho-$TARGET"
curl -fsSLO "$BASE/SHA256SUMS"
curl -fsSLO "$ASSETS"
sha256sum --check --ignore-missing SHA256SUMS
unzip -q "sacho-$VERSION-deploy-docs.zip" -d sacho-docs
sudo install -m 0755 "sacho-$TARGET" /usr/local/bin/sacho
sacho --version
```

**통과:** 체크섬 줄에 `OK`가 나오고 버전이 맞습니다. `SHA256SUMS`는 프로그램만 다루며 문서 묶음은
들어 있지 않습니다.

```sh
sudo useradd --system --home-dir /var/lib/sacho --shell /usr/sbin/nologin sacho
sudo install -d -m 0700 -o sacho -g sacho /var/lib/sacho /var/lib/sacho/secrets
sudo install -d -m 0750 -o root -g sacho /etc/sacho
sudo -u sacho sacho secrets init --vault /var/lib/sacho/secrets/secrets.enc \
  --key-file /var/lib/sacho/secrets/unlock.key
sudo install -m 0644 sacho-docs/examples/deploy/systemd/sacho.system.service /etc/systemd/system/sacho.service
```

## 3단계: 설정 쓰기

`sacho-docs/examples/deploy/sacho-production.toml`에서 시작합니다. 최상위 항목은 첫 `[섹션]` 위에
둡니다. 중계 서버가 아직 없으면 `[smtp]`를 지우고, `logout_url`은 인증 서비스의 로그아웃 경로로
바꾸거나 지웁니다.

```sh
sudo install -m 0640 -o root -g sacho sacho-docs/examples/deploy/sacho-production.toml /etc/sacho/sacho.toml
sudo sed -i \
  -e 's#^public_url = .*#public_url = "https://status.example.com"#' \
  -e 's#^admins = .*#admins = ["admin@example.com"]#' \
  /etc/sacho/sacho.toml
```

비밀값은 이름으로 저장합니다. 읽는 명령은 운영자의 비밀값 출처로 바꿉니다:

```sh
your-secret-reader | sudo -u sacho sacho secrets set SACHO_SMTP_PASSWORD \
  --vault /var/lib/sacho/secrets/secrets.enc --key-file /var/lib/sacho/secrets/unlock.key
```

예시에는 `probe_env`에 `API_AUTHORIZATION`이 있습니다. 점검이 쓸 이름으로 바꾸세요. 값이 없는 이름은
점검이 읽기 전까지 문제되지 않습니다.

```sh
sudo systemctl daemon-reload
sudo systemd-run --wait --pipe -q -p User=sacho -p WorkingDirectory=/var/lib/sacho \
  -p EnvironmentFile=-/etc/sacho/secrets.env \
  /usr/local/bin/sacho doctor --config /etc/sacho/sacho.toml
```

**통과:** `Configuration and database OK`. 오류는 문제를 알려 줍니다. `DEPLOY.ko.md` 11절의 표를 보세요.

## 4단계: 시작

```sh
sudo systemctl enable --now sacho
curl -fsS --retry 5 --retry-connrefused http://127.0.0.1:8080/healthz
```

**통과:** `{"status":"ok","version":"..."}`. 실패하면 `journalctl -u sacho -n 50`을 읽습니다.

## 5단계: 프록시 연결

`sacho-docs/examples/deploy/proxy/`에서 맞는 파일을 복사해 호스트 이름, 인증서 경로, 주소를 바꾸고
다시 읽힙니다. 다른 프록시는 `DEPLOY.ko.md` 3절의 경로 표를 지켜야 합니다.

운영자에게 시험용 세션 둘을 받습니다. 하나는 관리자, 하나는 다른 사람입니다. 쿠키 값은 비공개로
둡니다. 진짜 인증 서비스가 생기기 전에 연결을 시험하려면 `mock-verifier.py`를 loopback에서 돌리고
쿠키로 `session=admin`, `session=reader`를 쓴 뒤 끝나면 멈춥니다.

```sh
sh sacho-docs/examples/deploy/proxy/check-proxy.sh https://status.example.com \
  admin@example.com 'SESSION_COOKIE_OF_ADMIN' 'SESSION_COOKIE_OF_OTHER_PERSON'
```

헤더 이름을 바꿨으면 `IDENTITY_HEADER`, 사설 인증서면 `CURL_OPTS="--cacert FILE"`을 설정합니다.

**통과:** `All checks passed.`와 종료 상태 0. `FAIL`이 하나라도 있으면 진행하지 않습니다. 다른 기계에서
Sacho 포트가 답하면 안 됩니다.

## 6단계: 모니터 추가

```sh
cd sacho-docs/examples/deploy/checks
./register-check.py --admin-email admin@example.com your-check.json
```

스크립트는 검증하고 한 번 돌려 본 뒤 저장합니다. 여러 번 돌려도 안전합니다. 이름이 같은 모니터를
갱신하며, 따로 지시하지 않으면 장애(outage) 결과에서 멈춥니다. 적지 않은 항목은 기본값입니다. 한 항목만
손으로 바꾸려면 `GET /api/admin/items`의 `spec`을 읽어 고친 뒤 통째로 `/api/admin/items/ID`에 `PUT`합니다.
모르는 항목은 오류입니다.

자주 쓰는 API. 비공개 주소에서
`-H 'X-Forwarded-Email: ADMIN' -H 'X-Sacho-Request: 1' -H 'Content-Type: application/json'`을 붙입니다:

| 할 일 | 호출 |
| --- | --- |
| 모니터 목록 | `GET /api/admin/items` |
| 명세 검사 | `POST /api/admin/validate` |
| 저장 없이 한 번 실행 | `POST /api/admin/test` |
| 만들기; 교체 | `POST /api/admin/items`; `PUT /api/admin/items/ID` |
| 지금 실행; 결과 보기 | `POST /api/admin/items/ID/run`; `GET /api/admin/items/ID/logs` |
| 비밀 입력값 저장 | `PUT /api/admin/inputs/NAME`에 `{"value":"...","enabled":true,"encrypted":true}` |
| Push 키 발급(한 번만 보임) | `POST /api/admin/push/ID/token` |
| 페이지 배치 | `PUT /api/admin/layout`에 `{"sections":[{"title":"...","item_ids":["ID"]}]}` |
| 페이지 제목 | `PUT /api/admin/settings`에 `{"title":"...","description":"..."}` |
| 읽기 그룹 | `PUT /api/admin/access-groups`에 `[{"name":"ops","emails":["a@example.com"]}]` |
| 웹훅 추가 | `POST /api/admin/webhooks`에 `{"name":"...","url":"https://..."}` |
| 알림 시점 | `PUT /api/admin/notification-policy` |
| 워커 등록 | `POST /api/admin/worker-bootstrap`에 `{"id":"edge-west","ttl_seconds":3600,"allow_scripts":false}` |

비밀값이 든 본문은 인자가 아니라 표준 입력(`curl --data @-`)으로 보냅니다.

**통과:** 모니터마다 새 결과가 있습니다. 한 주기 뒤 `GET https://status.example.com/api/status`에 공개
모니터의 `status`가 `unknown` 아닌 값으로 나옵니다. 대기열에 들어간 것만으로는 성공이 아닙니다.

## 7단계: 제한 공개 시험

`public`이 아닌 모니터가 있으면 합니다. 무해한 시험 모니터에 `"visibility":"groups"`,
`"access_groups":["ops"]`를 설정하고 공개 주소에서 `/api/status`를 확인합니다:

| 요청 | 그 모니터는 |
| --- | --- |
| 세션 없음 | 없어야 함 |
| 세션 없이 위조한 신원 헤더 | 없어야 함 |
| 그룹 밖 사람의 세션 | 없어야 함 |
| 그룹 구성원의 세션 | 있어야 함 |

## 8단계: 메일 (설정한 경우)

지정된 시험용 메일함을 구독합니다:

```sh
curl -s -X POST https://status.example.com/api/subscribe -H 'Content-Type: application/json' \
  -H 'X-Sacho-Request: 1' -H 'Origin: https://status.example.com' -d '{"email":"test@example.com"}'
```

**통과:** 확인 메일이 옵니다. `GET /api/admin/notifications`에서 전달 결과를 봅니다.

## 9단계: 백업

`DEPLOY.ko.md` 8절의 명령을 원하는 타이머로 예약하고, 하나를 임시 경로로
`sacho database import`해 복구하고 `sacho database verify`를 돌립니다.

**통과:** 둘 다 성공하고, 설정·저장소·키도 백업 체계에 들어 있습니다.

## 10단계: 보고

비밀값 없이 한 일을 적습니다:

- 버전, 설치 방식, 서비스 단위와 설정 경로
- 공개 주소와 `check-proxy.sh` 결과
- 모니터 이름, ID, 최신 결과, 공개 범위
- 입력값 이름(값 없이)과 각각의 출처
- 메일과 자동 업데이트 상태
- 백업 위치와 복구 시험 결과
- 끝내지 못한 일과 운영자에게 필요한 것

## 이미 설치된 곳을 바꿀 때

1. `sacho --version`, `systemctl cat sacho`, 비밀값을 가리고 설정 읽기.
2. 위험한 일 전에 스냅숏 뜨기(`DEPLOY.ko.md` 8절).
3. 고치고 `doctor`를 돌린 뒤 재시작. `doctor`가 데이터베이스를 새 형식으로 바꿀 수 있으니 쓰려는 버전과
   경로로 돌립니다. 돌고 있는 서버와 충돌하지는 않습니다.
4. 프록시나 인증을 바꾼 뒤에는 `check-proxy.sh`를 다시 돌립니다.
5. 업그레이드는 워커까지 포함해 `DEPLOY.ko.md` 9절을 따릅니다.
