# 사초 v0.2.2 설치 후 새 기능 사용하기

Copyright Jioh L. Jung <jung@jioh.net>.

Linux x86_64와 ARM64용 완전 정적 musl 실행 파일을 제공합니다.
[설치 안내](DEPLOY.ko.md)에 따라 해당 CPU용 파일과 서명 매니페스트를 받으세요.

- **관련 오류 기록:** 경보·서비스·이력·그래프의 점을 누르면 화면 아래에 관련 경고·오류 패널이 열립니다. 검색·심각도·시간 범위를 조절할 수 있습니다. 원문은 관리자에게만 보이며, 인증 없는 운영 모드에서는 관리자 API의 기존 접근 규칙을 따릅니다. [사용법과 제한](EVENT-LOGS.ko.md)
- **바이너리 내장 안내:** `sacho agent-guide --language ko`로 설치 절차를 읽고, `sacho help --json`으로 명령과 설정 구조를 확인합니다. 실행 전 `sacho config check sacho.toml`, 운영 계정으로 `sacho doctor --config sacho.toml`을 실행하세요. doctor는 데이터베이스를 생성하거나 갱신할 수 있습니다. [에이전트 안내](AGENT-GUIDE.ko.md)
- **알림 선택:** 항목이나 경보의 **구독**에서 항목·그룹·이벤트·최소 심각도를 고릅니다. 메일의 **이 알림만 끄기**는 확인 후 해당 규칙만 끕니다. 기존 전체 수신 구독에서는 해당 항목·이벤트만 제외합니다. [구독 안내](SUBSCRIPTIONS.ko.md)
- **Grafana 직접 연결:** Prometheus 데이터 원천의 URL을 사초 주소로 지정하거나 Prometheus에서 `/metrics`를 수집합니다. 최소 PromQL 조회를 지원하며 완전한 Prometheus 저장소와 계산 결과가 항상 같지는 않습니다. 공개 항목의 익명 지표 읽기가 기본 활성화됩니다. 필요하면 `[metrics] anonymous_enabled = false` 또는 `enabled = false`로 제한하세요. 토큰 기본 범위는 그룹 항목을 제외하며, 과거 값은 추세 공개를 켠 항목에만 제공됩니다. 질의 시간·크기·동시 실행 제한도 적용됩니다. [지표·인증·질의 제한](METRICS.ko.md), [예제](../examples/grafana/)
- **설정 메뉴와 자연어 도우미:** 관리자의 설정 메뉴에서 변경안을 편집·검증하고 승인합니다. LLM 연결 후 자연어로 제안을 만들 수 있으며, 적용에는 관리자 승인이 필요합니다. 외부 LLM에 보내는 정보와 보관·되돌리기 절차를 먼저 확인하세요. [설정 안내](SETTINGS.ko.md)

v0.2.1 이후 서버 설치 안내도 보강했습니다. 공개 배포 문서에는 새 기능 설명, Grafana 설정·대시보드 예제와 배포 예제를 함께 제공합니다.
