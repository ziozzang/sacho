# 데이터베이스 이전과 분산 처리 기반

v0.1.4에는 네이티브 PostgreSQL 이전 어댑터와 버전이 있는 스냅샷이 있습니다.
**웹/API와 검사 스케줄러의 실행 저장소는 아직 SQLite입니다.** 기존 `database`
설정에 PostgreSQL URL을 넣거나 운영 저장소를 전환하지 마세요. SQL 데이터 조회
시나리오와 백엔드 저장소는 별개입니다.

```sh
sacho database export --database ./data.db --output ./backup.json
sacho database verify --input ./backup.json
sacho database import --input ./backup.json --database ./restored.db
```

내보내기는 WAL의 커밋된 변경을 포함하는 일관된 읽기 트랜잭션을 사용합니다.
복원은 새 파일에 적재하고 무결성 및 SHA-256 지문을 비교한 뒤 원자적으로
게시합니다. 기존 파일을 덮어쓰지 않습니다. 검사 ID·이력·구독 확인 토큰·알림
대기열과 재시도 상태를 보존하며, 알 수 없는 버전·테이블·칼럼은 거부합니다.

Unix에서는 스냅샷과 새 데이터베이스 권한이 0600입니다. **스냅샷은 평문이며
이메일·구독 토큰·검사 코드·로그를 담습니다.** Git과 공개 경로에 넣지 말고 백업
저장소를 보호하세요. 지문은 손상 검사이며 서명이 아닙니다. 최대 크기는 512 MiB입니다.
운영 전환 때는 원본 서버와 알림 워커를 중지하고 원본은 복구용으로 보존해야 합니다.
두 복사본을 동시에 실행하면 알림이 중복될 수 있습니다.

PostgreSQL 접속 URL은 비공개 환경변수 `SACHO_DATABASE_URL`로 주입합니다.
비밀번호를 명령 인자·예제·Git에 넣지 마세요. 원격 접속은 서버에 맞는 TLS
옵션(예: `sslmode=verify-full`)을 사용합니다. `psql` 없이 실행 파일 자체가 연결합니다.

```sh
sacho database import-postgres --input ./backup.json
sacho database export-postgres --output ./postgres-backup.json
sacho database verify --input ./postgres-backup.json
sacho database import --input ./postgres-backup.json --database ./returned.db
```

다른 변수는 `--url-env NAME`으로 선택합니다. 대상에는 `sacho` 네임스페이스가
없어야 하며 다른 네임스페이스는 변경하지 않습니다. 스키마 생성과 데이터 적재,
지문 검증을 하나의 트랜잭션으로 처리하고 실패하면 전체를 취소합니다.
자동 ID 시퀀스는 기존 ID 이후부터 이어집니다.

PostgreSQL 작업 임대 저장소는 선점·갱신·만료 후 인계·해제·펜싱을 구현했지만
`serve`에는 활성화하지 않았습니다. 데이터베이스 시간을 사용하고, 오래된 소유자의
반영을 차단하는 트랜잭션을 제공합니다. `sacho_coordination`의 임대는 일시적
소유권이며 업무 스냅샷에서 제외합니다. 전환 시 기존 노드를 모두 중지해야 하고
새 네임스페이스의 세대 ID가 오래된 임대 재사용을 차단합니다.
SQLite 중앙 서버의 원격 워커 API·노드 인증·작업 임대·결과 중복 차단은 구현했습니다.
PostgreSQL 다중 중앙 실행과 알림 선점은 아직 구현해야 합니다.
[분산 설계와 검증 기준](architecture/DISTRIBUTED.md)을 참고하세요.

```sh
cargo test --locked
python3 tests/storage_docker.py
```

테스트는 격리된 PostgreSQL 17 Docker 컨테이너와 임시 데이터, 동적 루프백 포트를
사용합니다. 왕복 이전·실패 시 취소·ID 연속성·동시 선점·인계·오래된 토큰 차단을
검증하고 컨테이너를 정리합니다. 운영 인증정보와 GitHub Actions를 사용하지 않습니다.
전체 분산 프로세스의 장애 테스트와 운영 전환은 아직 완료되지 않았습니다.

스냅샷 카탈로그 v2는 원격 실행과 영수증 테이블도 포함합니다. v1 복원은 빈 원격
테이블을 추가해 정규화하므로 지문이 바뀝니다. [원격 워커 안내](REMOTE-WORKERS.ko.md)를 참고하세요.
