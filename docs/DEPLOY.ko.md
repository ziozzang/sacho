# Sacho 배포와 설정

[English](DEPLOY.md) · [에이전트용 절차서](AGENT-GUIDE.ko.md) · [예시 파일](../examples/deploy/README.md)

이 안내서를 따르면 아무것도 없는 상태에서 공개 상태 페이지와 보호된 관리 화면까지 만들 수
있습니다. Sacho v0.2.2 기준입니다. 이름과 주소는 모두 예시이니 `status.example.com`,
`admin@example.com`, `127.0.0.1` 포트를 자기 값으로 바꾸세요.

더 긴 참고 문서는 프로그램에 들어 있어 인터넷 없이 볼 수 있습니다:
`sacho help setup`, `sacho help monitoring`, `sacho help workers`, `sacho help storage`.
한국어는 `--language ko`를 붙입니다.

## 구성 한눈에 보기

```text
방문자 ──HTTPS──> 역방향 프록시 ──> 127.0.0.1:8080의 Sacho ──> SQLite 파일
                      │                     ▲
                      ▼                     │ 바깥으로 나가는 연결만
               인증 서비스             원격 워커(선택)
```

- **Sacho는 실행 파일 하나입니다.** 웹 화면, Lua, SQLite, 암호화가 모두 들어 있습니다.
  Python과 셸 점검만 그 점검을 실행하는 기계의 `python3`와 `/bin/sh`를 씁니다.
- **Sacho는 직접 로그인을 처리하지 않습니다.** 역방향 프록시가 사람을 확인하고 이메일 주소를
  헤더로 넘깁니다. Sacho는 그 헤더를 그대로 믿으므로 **프록시 말고는 아무도 Sacho 포트에 닿으면
  안 됩니다.** 이 안내서에서 가장 중요한 규칙입니다.
- 페이지는 컨트롤러 하나가 제공합니다. 한 데이터베이스를 두 컨트롤러가 함께 쓸 수는 없습니다.

## 1. 설치

### 내려받고 확인하기

v0.2.1부터 Linux x86_64·ARM64용 바이너리는 모두 완전 정적 링크한 `musl` 판입니다.
CPU에 맞는 파일을 선택하면 glibc·OpenSSL 공유 라이브러리 없이 Linux에서 실행할 수 있습니다.
ARM64에서는 아래의 `TARGET`을 `aarch64-unknown-linux-musl`로 바꿉니다.

```sh
VERSION=v0.2.2
TARGET=x86_64-unknown-linux-musl
BASE=https://github.com/ziozzang/sacho/releases/download/$VERSION
curl -fsSLO "$BASE/sacho-$TARGET"
curl -fsSLO "$BASE/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS
sudo install -m 0755 "sacho-$TARGET" /usr/local/bin/sacho
sacho --version
```

### 먼저 내 컴퓨터에서 해 보기 (1분)

```sh
cp examples/deploy/sacho-minimal.toml sacho.toml
sacho doctor --config sacho.toml
sacho serve --config sacho.toml
```

`http://127.0.0.1:8080/`과 `/admin`을 엽니다. 이 방식에는 로그인이 없고, Sacho는 loopback
주소가 아니면 이 방식으로 시작하기를 거부합니다. 네트워크로 전달하지 마세요.

### 시스템 서비스 (권장)

전용 계정을 쓰고 자료는 `/var/lib/sacho`, 설정은 `/etc/sacho`에 둡니다:

```sh
sudo useradd --system --home-dir /var/lib/sacho --shell /usr/sbin/nologin sacho
sudo install -d -m 0700 -o sacho -g sacho /var/lib/sacho /var/lib/sacho/secrets
sudo install -d -m 0750 -o root -g sacho /etc/sacho

# 비밀값용 암호화 저장소를 만듭니다([secrets]를 쓰지 않으면 건너뜁니다).
sudo -u sacho sacho secrets init \
  --vault /var/lib/sacho/secrets/secrets.enc \
  --key-file /var/lib/sacho/secrets/unlock.key

# 설정 파일: 복사한 뒤 example.com 값을 모두 고칩니다.
sudo install -m 0640 -o root -g sacho examples/deploy/sacho-production.toml /etc/sacho/sacho.toml
sudo install -m 0600 examples/deploy/secrets.env.example /etc/sacho/secrets.env
sudo editor /etc/sacho/sacho.toml /etc/sacho/secrets.env

sudo install -m 0644 examples/deploy/systemd/sacho.system.service /etc/systemd/system/sacho.service
sudo systemctl daemon-reload

# 서비스와 같은 계정·환경으로 검사합니다.
sudo systemd-run --wait --pipe -q -p User=sacho -p WorkingDirectory=/var/lib/sacho \
  -p EnvironmentFile=-/etc/sacho/secrets.env \
  /usr/local/bin/sacho doctor --config /etc/sacho/sacho.toml

sudo systemctl enable --now sacho
curl -fsS --retry 5 --retry-connrefused http://127.0.0.1:8080/healthz    # {"status":"ok","version":"0.2.2"}
```

예시 파일은 `[smtp]`를 켜 두었고, 이 항목에는 실제로 찾을 수 있는 비밀번호 이름이 필요합니다.
메일 서버가 아직 없으면 처음에는 이 부분을 지우세요. `doctor`는 설정에 적힌 데이터베이스를
만들거나 새 형식으로 바꾸므로, 쓰려는 파일을 가리킬 때만 실행합니다.

### 사용자 서비스 (마지막 줄에만 root 필요)

```sh
mkdir -p ~/.local/bin ~/.config/sacho ~/.config/systemd/user ~/.local/share/sacho
install -m 0755 "sacho-$TARGET" ~/.local/bin/sacho
cp examples/deploy/sacho-production.toml ~/.config/sacho/sacho.toml   # 안의 경로는 절대 경로로
cp examples/deploy/systemd/sacho.user.service ~/.config/systemd/user/sacho.service
chmod 700 ~/.config/sacho
systemctl --user daemon-reload
systemctl --user enable --now sacho
sudo loginctl enable-linger "$USER"     # 로그아웃한 뒤에도 계속 실행
```

## 2. 설정 파일

Sacho는 `--config`로 지정한 TOML 파일 하나를 읽습니다(기본값 `sacho.toml`). 아래 항목은 필수라고
적힌 것 말고는 모두 생략할 수 있습니다. 모르는 이름은 오류로 처리되어 오타를 잡아 줍니다.
**최상위 항목은 첫 `[섹션]`보다 앞에 있어야 합니다.** `[auth]` 뒤에 두면
``unknown field `probe_env` `` 오류가 납니다. 바꾼 내용은 재시작해야 적용됩니다. 제목·설명·모니터·
배치·웹훅·그룹·입력값은 관리 화면에서 고치고 데이터베이스에 저장됩니다.

### 최상위

| 항목 | 기본값 | 뜻 |
| --- | --- | --- |
| `bind` | `127.0.0.1:8080` | 받을 주소와 포트. 비공개로 둡니다. |
| `database` | `sacho.db` | SQLite 파일. 절대 경로를 쓰고, 네트워크 공유가 아닌 로컬 디스크에 둡니다. |
| `public_url` | `http://127.0.0.1:8080` | 대표 공개 주소(경로 없이). 메일 링크와 Origin 검사에 씁니다. |
| `public_origins` | `[]` | 쓰기를 추가로 허용할 정확한 주소. 최대 16개, 와일드카드 불가. 예: `["https://monitor.example.com"]`. |
| `title` | `Sacho` | 처음 페이지 제목. |
| `description` | `Service health, recorded with care.` | 처음 페이지 설명. |
| `retention_days` | `100` | 관측 결과 보관 일수. 90~3650. |
| `max_concurrent_checks` | `16` | 동시에 도는 점검 수. 1~128. |
| `probe_env` | `[]` | 점검이 읽어도 되는 환경 변수 이름. 최대 32개. `PATH`, `LANG`, `SACHO_PROBE_*` 제외. |

### `[auth]`

| 항목 | 기본값 | 뜻 |
| --- | --- | --- |
| `mode` | `no-auth` | `no-auth`(loopback 전용) 또는 `auth-proxy`. |
| `email_header` | `X-Forwarded-Email` | 확인된 이메일이 담기는 헤더. |
| `admins` | `[]` | 관리자 주소. `auth-proxy`에서 필수. 대소문자만 무시하고 정확히 일치해야 하며, 와일드카드·도메인 일치는 없습니다. |
| `logout_url` | 없음 | 프록시 로그아웃 페이지의 로컬 경로나 `https://` 주소. 없으면 로그아웃 링크가 나오지 않습니다. |

### `[smtp]` (선택. 없으면 메일 구독이 꺼졌다고 화면에 표시됩니다)

| 항목 | 기본값 | 뜻 |
| --- | --- | --- |
| `host` | 필수 | 메일 중계 서버. |
| `port` | `587` | 중계 서버 포트. |
| `from` | 필수 | 보내는 사람. 예: `Status <status@example.com>`. |
| `username` | 없음 | 중계 서버 로그인 이름. |
| `password_env` | 없음 | 비밀번호가 든 환경 변수나 저장소 항목의 **이름**. 찾을 수 없으면 Sacho가 시작하지 않습니다. |
| `security` | `starttls` | `starttls`, `tls`(보통 465번 포트), `plain`(loopback, `localhost`, 사설 IPv4 주소 전용). |

### `[updates]`

| 항목 | 기본값 | 뜻 |
| --- | --- | --- |
| `enabled` | `false` | 서명된 배포판을 자동 설치하고 작업 프로세스를 재시작합니다. |
| `interval_seconds` | `21600` | 확인 주기. 최소 60. |
| `public_key` | 내장 | 직접 서명한 빌드를 쓸 때의 base64 Ed25519 공개 키. |
| `manifest_url` | GitHub | 서명된 목록 파일의 자체 HTTPS 주소. |
| `ca_file` | 없음 | `manifest_url`용 사설 인증 기관. |

### `[secrets]` (선택. 쓰면 둘 다 필수)

| 항목 | 뜻 |
| --- | --- |
| `vault` | `sacho secrets init`이 만드는 암호화 파일. 관리 화면에서 쓰기를 하므로 그 폴더에 서비스 계정이 쓸 수 있어야 합니다. |
| `key_file` | 무작위 잠금 해제 키. 서비스 계정 소유이고 그룹·다른 사용자 권한이 없어야 합니다. |

이름이 같으면 저장소보다 환경 변수 값이 우선합니다.

### `[[remote_workers]]` (예전 수동 워커 등록)

여기에 아무것도 적을 필요가 없는 일회용 등록(7절)을 권합니다. 워커마다 블록 하나, 최대 128개:
`id`(필수), `token_env`(필수. 32~512자 인쇄 가능 문자로 된 고유 비밀값의 이름, `probe_env`에 넣지
않음), `allow_scripts`(기본 `false`).

### 워커 설정

| 항목 | 기본값 | 뜻 |
| --- | --- | --- |
| `control_url` | 필수 | 컨트롤러 주소. HTTPS. 일반 HTTP는 loopback에서만. |
| `node_id` | 필수 | 이 워커의 이름. 등록 키와 일치해야 합니다. |
| `token_env` | 필수 | 자격 증명을 저장할 이름. `probe_env`에 넣지 않습니다. |
| `label` | 비어 있음 | 관리 화면에 보이는 이름. |
| `probe_env` | `[]` | 이 워커의 점검이 읽을 입력값. 컨트롤러 값은 공유되지 않습니다. |
| `allow_scripts` | `false` | 여기서 Lua·셸·Python 실행을 허용. 컨트롤러도 허용해야 합니다. |
| `ca_file` | 없음 | 컨트롤러 인증서를 발급한 사설 인증 기관. |
| `allow_insecure_http` | `false` | loopback이 아닌 컨트롤러에 일반 HTTP로 연결. 격리된 시험 전용. |
| `controller_id`, `[secrets]` | `worker-join`이 기록 | 손으로 적지 않습니다. |

### 로그

로그는 표준 출력으로 나가므로 `journalctl -u sacho`로 봅니다. 자세한 정도는 `RUST_LOG` 환경
변수로 바꿉니다. 예: `RUST_LOG=sacho=debug`.

## 3. 역방향 프록시

실제로 시험한 설정: [nginx](../examples/deploy/proxy/nginx-sacho.conf),
[Caddy](../examples/deploy/proxy/Caddyfile),
[Traefik](../examples/deploy/proxy/traefik-dynamic.yml). 이 설정들은 oauth2-proxy 방식의 인증
서비스를 전제로 합니다. 즉 `GET /oauth2/auth`가 유효한 세션에는 2xx와 `X-Auth-Request-Email`을,
아니면 401을 돌려줍니다.

### 어떤 프록시든 지켜야 할 것

| 경로 | 접근 | Sacho로 보내는 신원 헤더 |
| --- | --- | --- |
| `/admin` | 세션 필수. 브라우저는 로그인으로 보냄 | 확인된 이메일 |
| `/api/admin/` | 세션 필수. 리다이렉트가 아닌 401로 응답 | 확인된 이메일 |
| `/api/account` | 세션 필수. 리다이렉트가 아닌 401로 응답 | 확인된 이메일 |
| `/api/status`, `/api/incidents`, `/api/history/` | 공개, 신원은 있으면 전달 | 로그인했으면 이메일, 아니면 없음 |
| `/api/push/`, `/api/workers/` | 공개. 자체 bearer 자격 증명을 씀 | 없음 |
| 나머지 (`/`, `/assets/`, `/healthz`, `/api/subscribe`, `/subscription/`) | 공개 | 없음 |

그리고 **모든** 경로에서:

1. **클라이언트가 보낸 신원 헤더를 지웁니다.** 공개 경로도 마찬가지입니다. 상태 자료는 사람마다
   걸러서 보여 주므로, 공개 경로에서 위조를 허용하면 비공개 모니터가 드러납니다.
2. 헤더는 최대 한 번만 넣습니다. 값이 두 개면 신원이 없는 것으로 처리됩니다.
3. 로그인 쿠키를 Sacho로 넘기지 않습니다. Sacho는 쿠키를 쓰지 않습니다.
4. HTTP는 HTTPS로 넘기고, `Strict-Transport-Security`는 프록시에서 넣습니다.
5. 워커와 push 보고를 로그인 페이지로 보내지 않습니다. 워커는 리다이렉트를 오류로 처리합니다.

인증 서비스가 애플리케이션 주소에서 세션을 넘겨받는 방식이면, 로그인·로그아웃 페이지도 예시의
`/oauth2/`처럼 이 호스트 이름으로 연결해야 합니다.

모니터가 모두 공개라면 "신원은 있으면 전달" 줄은 없애도 됩니다. 그 경로들은 그냥 공개 경로가 되고,
로그인한 사람에게도 다른 방문자와 같은 화면이 보입니다.

### nginx

```sh
sudo cp examples/deploy/proxy/nginx-sacho.conf /etc/nginx/conf.d/sacho.conf
sudo editor /etc/nginx/conf.d/sacho.conf
sudo nginx -t && sudo systemctl reload nginx
```

이 파일은 신원 헤더를 server 수준에서 한 번만 설정하고, 그 값은 확인을 거친 location만 채우는
변수에서 옵니다. nginx에서는 location 안에 `proxy_set_header`를 하나라도 쓰면 위에서 물려받은 것이
모두 사라지므로, 다른 줄을 함께 적지 않고 location에 추가하면 안 됩니다.

### Caddy

```sh
sudo cp examples/deploy/proxy/Caddyfile /etc/caddy/Caddyfile
sudo editor /etc/caddy/Caddyfile
sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy
```

### Traefik

[traefik-dynamic.yml](../examples/deploy/proxy/traefik-dynamic.yml)을 file provider로 읽힙니다.
Traefik은 헤더 이름을 바꿀 수 없으므로 Sacho에 `email_header = "X-Auth-Request-Email"`로 설정합니다.
로그인한 사람에게 제한된 모니터를 보여 주려면, 항상 200으로 답하고 유효한 세션일 때만 이메일을 붙이는
확인 주소가 인증 서비스에 있어야 합니다. 없을 때 해당 라우터를 빼는 방법은 파일 주석에 있습니다.

### 인증 서비스

forward-auth 확인에 답할 수 있는 서비스면 무엇이든 됩니다. oauth2-proxy는 아래 옵션으로 위 방식이
됩니다. 제공자 설정은 쓰는 신원 제공자에 따라 다릅니다:

```sh
oauth2-proxy --http-address=127.0.0.1:4180 --reverse-proxy=true --set-xauthrequest=true \
  --upstream=static://202 --redirect-url=https://status.example.com/oauth2/callback \
  --email-domain='*'   # 여기에 --provider, 클라이언트 ID, 쿠키 비밀값 설정을 더합니다
```

`--email-domain='*'`이면 그 제공자의 어떤 계정이든 로그인할 수 있습니다. 관리자 권한은 `admins`에
적힌 주소만 받으므로 Sacho 판단에는 문제가 없지만, 로그인한 누구나 `authenticated` 공개 범위의
모니터를 볼 수 있습니다. 그것이 곤란하면 범위를 좁히세요.

[시험용 확인기](../examples/deploy/proxy/mock-verifier.py)는 연결만 시험하는 동안 실제 서비스를
대신합니다. 진짜 로그인이 없으니 절대 외부에 열지 마세요.

### 확인하기

[check-proxy.sh](../examples/deploy/proxy/check-proxy.sh)는 신원 경계를 바깥에서 시험하며, 서버의
내용은 바꾸지 않습니다:

```sh
sh examples/deploy/proxy/check-proxy.sh https://status.example.com admin@example.com \
   'your_session_cookie=ADMIN_VALUE' 'your_session_cookie=READER_VALUE'
```

다른 기계에서 Sacho 자체 포트가 답하지 않는지도 확인합니다:
`curl --max-time 5 http://YOUR_SERVER:8080/healthz`는 실패해야 합니다.

이 세 설정과 스크립트는 Sacho v0.2.0에서 nginx(Debian 13), Caddy 2.11, Traefik 3.7과 시험용 확인기로
돌려 보았습니다. 실제 신원 제공자는 이 시험에 포함되지 않았습니다.

## 4. 인증과 접근 권한

- **관리자:** `admins`에 있는 값을 헤더로 가지고 온 사람입니다. 바꾸려면 목록을 고치고 재시작합니다.
  관리자는 스크립트 점검으로 서버에서 명령을 실행할 수 있으니 셸 권한처럼 다루세요.
- **신원으로 인정되는 것:** 헤더 값이 정확히 하나이고, 254자 이하의 올바른 이메일 주소일 때입니다.
  소문자로 바꿔 비교합니다. 그 밖에는 익명입니다.
- **모니터 읽기:** 모니터마다 `visibility`가 있습니다. `public`(기본값), `authenticated`(로그인한
  누구나), `groups`(이름 붙인 그룹의 구성원. 관리 화면 **Settings → Data access groups**에서
  설정). 요청마다 확인하므로 구성원에서 빼면 바로 접근이 끊깁니다. 숨은 모니터는 페이지, 전체 상태,
  공개 메일에서 빠집니다.
- **쓰기:** 모든 변경에 `X-Sacho-Request: 1` 헤더가 필요하고, 브라우저의 `Origin`은 `public_url`이나
  `public_origins`와 일치해야 합니다. 다른 사이트에서 위조한 요청을 막습니다.
- **호스트 이름 추가:** 프록시와 인증 서비스, `public_origins`에 추가합니다. 메일에 쓰는 이름은
  `public_url` 하나입니다.

## 5. 메일과 웹훅

```toml
[smtp]
host = "smtp.example.com"
port = 587
from = "Example Status <status@example.com>"
username = "status@example.com"
password_env = "SACHO_SMTP_PASSWORD"
security = "starttls"
```

비밀번호는 둘 중 한 곳에 넣고 재시작합니다:

```sh
# 암호화 저장소. 명령줄이 아니라 비밀번호 관리 도구에서 읽어 넘깁니다.
your-secret-reader | sudo -u sacho sacho secrets set SACHO_SMTP_PASSWORD \
  --vault /var/lib/sacho/secrets/secrets.enc --key-file /var/lib/sacho/secrets/unlock.key
```

또는 비공개 파일 `/etc/sacho/secrets.env`에 `SACHO_SMTP_PASSWORD=...`를 넣습니다. 이 이름은
`probe_env`에 넣지 않습니다. 상태 페이지에서 시험용 메일함을 구독해 확인합니다. 확인 메일이 와야
하며 링크는 24시간 유효합니다. 확인 메일은 전체 시간당 100건, 주소당 시간당 1건으로 제한됩니다.

공개 범위가 제한된 모니터는 공개 구독자에게 메일을 보내지 않습니다. **Notifications**에서 웹훅
등록, 메시지 틀 편집과 미리 보기, 알림 지연(`settle_seconds`), 반복 제한(`cooldown_seconds`), 계획된
점검 중 알림 끄기를 할 수 있습니다. 웹훅은 중복 전달될 수 있으니 받는 쪽은 같은 `Idempotency-Key`를
무시해야 합니다.

## 6. 모니터 추가

`/admin` 화면이나 JSON API를 씁니다. 자동 설정에는 API를 권합니다. 공개 주소에서는 API에 진짜 세션이
필요합니다. Sacho가 도는 기계에서는 비공개 주소가 신원 헤더를 바로 받습니다:

```sh
curl -s http://127.0.0.1:8080/api/admin/items -H 'X-Forwarded-Email: admin@example.com'
```

[register-check.py](../examples/deploy/checks/register-check.py)는 점검을 검증하고 한 번 돌려 본 뒤,
새로 만들거나 이름이 같은 것을 갱신합니다. 이력은 유지됩니다:

```sh
cd examples/deploy/checks
./register-check.py --admin-email admin@example.com http-json-value.json
./register-check.py --admin-email admin@example.com lua.json
./register-check.py --admin-email admin@example.com python.json
./register-check.py --admin-email admin@example.com push.json
```

| 예시 | 보여 주는 것 |
| --- | --- |
| [http.json](../examples/deploy/checks/http.json) | 응답이 느리면 경고하고 짧은 실패는 넘어가는 웹사이트 점검 |
| [http-json-value.json](../examples/deploy/checks/http-json-value.json) | JSON 값 하나를 공개 표시 |
| [lua.json](../examples/deploy/checks/lua.json), [lua-scenario.lua](../examples/deploy/checks/lua-scenario.lua) | 내용까지 확인하는 여러 요청. 설치할 것 없음 |
| [python.json](../examples/deploy/checks/python.json), [python-disk.py](../examples/deploy/checks/python-disk.py) | 경고·위험 기준과 그래프가 있는 로컬 스크립트 |
| [push.json](../examples/deploy/checks/push.json), [push-report.sh](../examples/deploy/checks/push-report.sh) | 작업이 직접 보고. 소식이 끊기면 실패 |

DNS, TCP, ping, SNMP, Prometheus 점검도 내장되어 있습니다. `sacho help monitoring`과
`sacho template --help`를 보세요.

**점검의 비밀값.** 이름을 `probe_env`에 적거나(또는 관리 화면 **Environment & secrets**에 추가하고)
Lua는 `sacho.env("NAME")`, Python은 `os.environ["NAME"]`, 셸은 `"$NAME"`로 읽습니다. HTTP 점검은
`"headers_env": {"Authorization": "NAME"}`을 쓰며, 값에는 `Bearer `까지 헤더 전체가 들어갑니다.

**Push 보고.** 모니터를 저장한 뒤 키를 발급합니다. 키는 한 번만 보여 줍니다:

```sh
curl -s -X POST http://127.0.0.1:8080/api/admin/push/MONITOR_ID/token \
  -H 'X-Forwarded-Email: admin@example.com' -H 'X-Sacho-Request: 1'
```

`token`을 보고하는 기계의 비공개 파일에 저장하고, 작업에서
`push-report.sh https://status.example.com MONITOR_ID /path/to/token 12.5`를 부릅니다.
`interval_seconds`(최대 86400)는 소식 없이 허용되는 가장 긴 시간입니다. 하루 한 번 도는 작업은
실행 시간 변화를 고려해 `failure_threshold`를 올리거나 더 자주 보고하세요.

## 7. 원격 워커

워커는 다른 네트워크에서 점검합니다. 바깥으로 나가는 연결만 쓰고 데이터베이스나 열린 포트가 필요
없습니다. 컨트롤러와 같은 버전의 실행 파일을 쓰세요.

1. 관리 화면 **Agents**에서 노드 이름, 유효 기간, 스크립트 허용 여부를 정해 일회용 키를 만듭니다.
   API로는 `POST /api/admin/worker-bootstrap`에
   `{"id":"edge-west","ttl_seconds":3600,"allow_scripts":false}`를 보냅니다.
2. 워커 기계에 실행 파일을 설치하고 1절처럼 계정과 `/var/lib/sacho`를 만듭니다.
   [worker.toml](../examples/deploy/worker.toml)을 `/var/lib/sacho/worker.toml`로 복사해 그 계정
   소유, 권한 0600으로 둡니다. `control_url`, `node_id`, `label`을 적습니다.
3. 등록합니다. 키는 셸 기록에 남지 않도록 표준 입력으로 넘깁니다:

   ```sh
   read -rs key                        # 붙여 넣고 Enter
   printf '%s' "$key" | sudo -u sacho sacho worker-join --config /var/lib/sacho/worker.toml
   unset key
   ```

   그 폴더에 해당 계정이 쓸 수 있어야 합니다. 등록할 때 설정 옆에 암호화된 자격 증명을 만들고
   설정 파일을 다시 씁니다.
4. [sacho-worker.service](../examples/deploy/systemd/sacho-worker.service)를 실행합니다.
5. 모니터의 **Execution location**에서 워커를 고릅니다(API는 `"worker": "edge-west"`).

사설 인증 기관은 `ca_file`에 넣습니다. 서버 인증서를 실제로 발급한 인증 기관이어야 하며, 자체
서명 서버 인증서를 넣으면 `CaUsedAsEndEntity`로 실패합니다. 워커는 스스로 업데이트하지 않으니
컨트롤러와 함께 올리세요. 연결을 끊으려면 **Agents**에서 폐기합니다.

## 8. 백업과 복구

아래를 함께 백업하되, 데이터베이스 스냅숏·저장소와 잠금 해제 키는 따로 보관합니다:

| 대상 | 위치(이 안내서 기준) | 이유 |
| --- | --- | --- |
| 설정 | `/etc/sacho/sacho.toml`, `/etc/sacho/secrets.env` | 데이터베이스에 없음 |
| 데이터베이스 스냅숏 | 아래 | 모니터, 스크립트, 이력, 구독자 |
| 암호화 저장소와 키 | `/var/lib/sacho/secrets/` | 없으면 암호화된 입력값을 읽을 수 없음 |

스냅숏은 서비스가 돌고 있어도 안전합니다:

```sh
sudo install -d -m 0700 -o sacho -g sacho /var/backups/sacho
FILE=/var/backups/sacho/sacho-$(date -u +%Y%m%dT%H%M%SZ).json
sudo -u sacho sacho database export --database /var/lib/sacho/sacho.db --output "$FILE"
sudo -u sacho sacho database verify --input "$FILE"
```

스냅숏은 스크립트, 로그, 구독자 주소가 들어 있는 평문이므로 잘 보호하세요. 있는 파일은 덮어쓰지
않습니다. 스냅숏은 최대 512 MiB입니다. 데이터베이스가 더 크면 파일로 복사합니다:
`sqlite3 sacho.db ".backup /path/copy.db"`, 또는 서비스를 멈추고 복사. 돌고 있는 데이터베이스를
그냥 `cp`하지 마세요.

복구(있는 데이터베이스는 덮어쓰기를 거부합니다):

```sh
sudo systemctl stop sacho
sudo -u sacho mv /var/lib/sacho/sacho.db /var/lib/sacho/sacho.db.before-restore
sudo -u sacho sacho database import --input "$FILE" --database /var/lib/sacho/sacho.db
sudo systemctl start sacho
sudo -u sacho sacho doctor --config /etc/sacho/sacho.toml
```

옛 데이터베이스 옆에 `-wal`, `-shm` 파일이 있으면 함께 옮기세요. 페이지가 정상으로 보일 때까지 옛
사본을 지우지 마세요. 옛 서버와 새 서버를 함께 돌리면 구독자가 메일을 중복으로 받을 수 있습니다.

## 9. 업그레이드와 되돌리기

먼저 스냅숏을 뜨고(8절) 배포 안내를 읽으세요.

**직접 교체**는 가장 단순하고 `/usr/local/bin`에 맞습니다:

```sh
sudo install -m 0755 /usr/local/bin/sacho /usr/local/bin/sacho.previous
# 1절처럼 새 버전을 내려받아 확인한 뒤:
sudo install -m 0755 "sacho-$TARGET" /usr/local/bin/sacho
sudo systemctl restart sacho
curl -fsS --retry 5 --retry-connrefused http://127.0.0.1:8080/healthz
```

**`sacho update`**는 서명, 체크섬, 대상 플랫폼을 대신 확인합니다. 데이터베이스 잠금을 잡으므로
서비스를 멈춰야 하고, 실행 파일이 그 계정이 쓸 수 있는 폴더에 있어야 합니다:

```sh
sacho update --check     # 확인만. 설정 파일 불필요
sudo systemctl stop sacho
sudo -u sacho sacho update --config /etc/sacho/sacho.toml
sudo systemctl start sacho
```

옛 프로그램은 새 프로그램 옆에 `.sacho-rollback-XXXXXX`로 남고, 이름을 알려 줍니다.

**자동 업데이트**(`[updates] enabled = true`)에도 쓰기 가능한 실행 파일 폴더가 필요합니다.
`/var/lib/sacho/bin`에 설치하고 `ExecStart`를 고치세요. 새 버전이 시작에 실패하거나 30초 안에
끝나면 옛 버전으로 돌아갑니다. 이 30초 동안 전원이 나가면 새 버전이 돌고 되돌리기 파일만 남을 수
있습니다.

**되돌리기.** 서비스를 멈추고 옛 실행 파일을 제자리에 놓은 뒤 시작합니다. 데이터베이스 형식이 더
새롭다며 옛 버전이 열기를 거부하면, 업그레이드 전에 뜬 스냅숏을 옛 실행 파일로 복구합니다(8절).
그 뒤의 관측 결과는 사라집니다. 옛 워커는 새 모니터 설정을 거부할 수 있으니 컨트롤러와 함께 올리세요.

## 10. 보안 점검표

- [ ] Sacho 포트에는 프록시만 닿습니다. **다른 사용자와 함께 쓰는 기계에서는 어떤 로컬 프로세스든
      loopback 포트에 닿아 관리자라고 주장할 수 있습니다.** 그런 곳에서는 전용 컨테이너나 네트워크
      네임스페이스에서 돌리거나 방화벽의 소유자 규칙을 쓰세요.
- [ ] 프록시가 모든 경로에서 클라이언트의 신원 헤더를 지웁니다. `check-proxy.sh`를 통과합니다.
- [ ] 프록시와 Sacho가 다른 기계에 있으면 그 사이 연결이 비공개이거나 암호화되어 있습니다.
- [ ] `no-auth`는 로컬 평가에만 씁니다.
- [ ] Sacho는 sudo 권한 없는 전용 계정으로 돕니다. 스크립트 점검은 그 계정 권한으로 돌고, 믿을 수
      있는 관리자만 만들 수 있습니다.
- [ ] 꼭 필요할 때만 Docker 소켓 권한을 줍니다. 읽기만 해도 호스트를 제어할 수 있습니다.
- [ ] `sacho.toml`, `secrets.env`, 저장소, 키, 스냅숏은 비공개이고 Git 밖에 있습니다.
- [ ] 비밀번호를 TOML, 점검 스크립트, URL, 명령줄 인자에 넣지 않습니다.
- [ ] 내부 이름이나 대상이 드러나는 새 모니터는 `authenticated`나 `groups`로 둡니다. 기본값은
      `public`입니다. 공개하기로 고른 값만 표시됩니다.
- [ ] 컨트롤러 관리자가 그 기계에서 코드를 실행해도 된다고 믿을 때만 워커의 `allow_scripts`를 켭니다.
- [ ] 백업은 실제로 복구해 보아 확인합니다.

## 11. 문제 해결

| 증상 | 원인과 해결 |
| --- | --- |
| 시작할 때 ``unknown field `...` `` | 오타이거나 최상위 항목이 `[섹션]` 뒤에 있습니다. 위로 옮기세요. |
| `no-auth must bind to loopback` | 다른 주소에는 `auth-proxy`를 쓰세요. |
| `auth-proxy requires admin email addresses` | `[auth].admins`를 채우세요. |
| `SMTP password environment variable is missing` | `password_env`의 이름이 서비스 환경에도 저장소에도 없습니다. 내 셸의 변수는 해당되지 않습니다. |
| `plain SMTP is only allowed on loopback or private IPv4 networks` | `starttls`나 `tls`를 쓰거나, IP 주소로 적은 사설 중계 서버를 쓰세요. |
| `sacho init`의 `File exists` | 절대 덮어쓰지 않습니다. 있는 파일을 쓰거나 다른 경로를 주세요. |
| `another Sacho server or update is using this database` | 서비스를 먼저 멈추거나 두 번째 실행본이 있는지 보세요. |
| 저장소를 읽지 못함 | 파일이 서비스 계정 소유이고 그룹·다른 사용자 권한이 없어야 합니다. |
| 관리 화면에서 비밀값 저장 실패 | 저장소 폴더에 서비스가 쓸 수 있어야 합니다. 단위 파일의 `ReadWritePaths`를 확인하세요. |
| 관리 API 401 | 유효한 신원이 없습니다. 세션 만료, `admins`에 없는 주소(정확히 일치), 프록시가 헤더를 넘기지 않음 중 하나입니다. 세션을 넣어 `/api/account`를 부르면 Sacho가 받은 이메일이 보입니다. |
| 관리 API 403 | `X-Sacho-Request: 1`이 없거나 `Origin`이 `public_url`/`public_origins`에 없습니다. |
| 구독할 때 `{"error":"X-Sacho-Request: 1 required."}` | `/api/subscribe`에도 같은 헤더가 필요합니다. |
| API 400 | 잘못된 항목. 흔한 경우: `timeout_seconds`가 `interval_seconds`보다 큼, `interval_seconds`가 5~86400 밖. `POST /api/admin/validate`가 오류를 알려 줍니다. |
| API 429 | 동시 시험 실행이 너무 많습니다. 기다렸다가 다시 하세요. |
| 로그인한 사람에게 제한된 모니터가 안 보임 | 프록시의 상태 경로에 "신원은 있으면 전달"이 없습니다. 3절을 보세요. |
| 모니터가 계속 `unknown` | 아직 결과가 없습니다. Push는 아직 보고가 없는 것이고, 워커 모니터는 워커가 연결되지 않은 것입니다. |
| 점검이 값을 읽지 못함 | 그 이름이 점검을 실행하는 기계의 `probe_env`와 서비스 환경에 모두 있어야 합니다. |
| 쓰지도 않는 이름에 `NAME: not in probe_env` Lua 경고 | 검증이 주석 안의 `sacho.env("...")`도 읽습니다. 주석을 고치세요. |
| 워커 시작 때 `named input is not available` | 아직 등록하지 않았습니다. `worker-join`을 먼저 하세요. |
| `worker-join`의 `read a one-time bootstrap key from stdin` | 등록이 안 되어 키가 필요합니다. 연결 실패로는 키가 소모되지 않습니다. 만료되었으면 새로 발급하세요. |
| 워커의 `CaUsedAsEndEntity` | `ca_file`이 자체 서명 서버 인증서입니다. 인증 기관과 그 기관이 발급한 서버 인증서를 쓰세요. |
| 워커가 리다이렉트 오류로 실패 | 프록시가 `/api/workers/`를 로그인으로 보냈습니다. 그 경로를 공개로 두세요. |
| 호스트는 살아 있는데 ping 점검 실패 | 기계에 Linux ping 소켓(`net.ipv4.ping_group_range`)이나 raw 소켓 권한이 필요합니다. |
| 데이터베이스가 계속 커짐 | `retention_days`를 낮추거나(최소 90) 점검 주기를 늘리세요. 정리는 매시간 돕니다. |


## 에이전트로 설정하기 / Configure with an agent

The executable includes its own runbook: `sacho agent-guide` (or `--language ko`).
Use `sacho help --json` for versioned command/options/configuration metadata,
`sacho config check sacho.toml` before starting, and `sacho doctor --config
sacho.toml` for environment checks. For worker files add `--worker` to check and
doctor. Config check leaves the database untouched; doctor may create/migrate it.
Run as the service user with the actual environment and working directory.
The embedded [configuration reference](CONFIGURATION.md) lists every server and
worker TOML key, defaults and named environment inputs. Pass secrets using private
files, environment or stdin, never command arguments or a report.
