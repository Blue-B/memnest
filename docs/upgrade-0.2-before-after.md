# memnest 0.2 변경 전후

이 문서는 0.2 구현을 시작하기 직전의 실제 로컬 상태와 구현 후 검증 결과를 기록한다.

## 화면

| 변경 전 | 변경 후 |
| --- | --- |
| [`dashboard-before.png`](./dashboard-before.png) | [`dashboard.png`](./dashboard.png) |
| 장식용 배경, 캔버스 노드 애니메이션, 큰 소개 영역 중심 | 검색, 지연, 작업, 실패, 피드백, 데이터 편중 중심의 운영 콘솔 |
| `/assets/memory-atlas.png`, `/favicon.ico` 404 발생 | 브라우저 콘솔 오류 0건 |

## 기능 비교

| 영역 | 변경 전 | 변경 후 |
| --- | --- | --- |
| 대시보드 주소 | API는 3111인데 사용되지 않는 viewer 포트 3113도 노출 | API와 대시보드를 같은 포트로 명시, `memnest status`, `memnest dashboard`, pi `/memnest` 제공 |
| 검색 관측 | 결과와 시간만 일회성 반환 | 검색어, 범위, 결과 ID, 지연, 어댑터, 판정을 90일간 로컬 기록 |
| 저장 관측 | 백그라운드 저장 성공 여부를 호출자가 알기 어려움 | 모든 저장에 job ID와 succeeded, deduplicated, failed 상태 기록 |
| 재시작 안전 | 저장을 승인한 뒤 프로세스가 종료되면 기록 손실 가능 | 저장 완료 후 승인, 재시작 시 중단된 job을 failed로 표시 |
| 의미 중복 | 반환된 ID가 실제로 존재하지 않는 유령 ID 가능 | 별칭을 canonical 메모리에 연결해 반환 ID로 조회 가능 |
| 한글 컨텍스트 | UTF-8 바이트를 글자 수처럼 계산 | Unicode 문자 수 기준 예산과 한글 회귀 테스트 |
| 메모리 모델 | chunk type과 importance 중심 | record, fact, rule, procedure, confidence, source IDs, supersedes, verified at 추가 |
| 검색 품질 피드백 | 없음 | recall ID에 helpful, harmful, ignored 입력, 재시도와 판정 변경을 원자적으로 반영 |
| 피드백의 랭킹 반영 | 피드백이 기록과 표시까지만 | helpful, harmful가 검색 점수에 포화 가중치(±0.10)로 반영되어 실제로 쓰인 기억이 상위로 올라오는 폐루프 완성 |
| 플랫폼 | pi 도구와 범용 MCP | pi 최우선 지원, 범용 MCP 확장, 공개 어댑터 계약, 의존성 없는 JSONL HTTP 참조 구현 |
| 인증 | 일반 호출 일부만 토큰 적용 | pi 도구, Autocontext, AutoLog 모두 `MEMNEST_TOKEN` 전달 |
| 기존 데이터 경로 | 기본값과 문서가 불일치 | 새 설치는 `~/.memnest`, 기존 `~/.factory/memories`만 있으면 자동 보존 |

## 수정 전 기준

- 코어 버전: 0.1.0
- pi 확장 버전: 0.5.2
- 실제 저장량: 26,998 chunks, 204 sessions
- root 집중: 25,260 chunks
- 30일 초과 root 기록: 17,086 chunks
- 브라우저 콘솔: 404 오류 2건
- 기준 테스트: core 46, pi 52, learn 41, journal 14 통과

## 수정 후 검증

- 코어 버전: 0.2.0
- pi 확장 버전: 0.6.0, 20 tools
- core: 50 library tests와 1 CLI test 통과
- pi: 56 smoke assertions와 인증 및 명령 5 assertions 통과
- MCP end to end: 11 assertions 통과
- learn: 41 tests 통과
- journal: 14 smoke assertions 통과
- generic HTTP adapter: 11 assertions 통과
- 피드백 폐루프: 실제 HTTP 검색과 helpful 피드백 3회로 helpful_count 저장과 점수 0.35 -> 0.40 상승 확인 (feedback_bonus(3,0)=0.05와 일치)
- 브라우저: 대시보드, 검색, 피드백, 키보드 포커스 흐름 확인
- 브라우저 콘솔: 오류 0건, 경고 0건

초기 문서 변경은 구현 전부터 작업 트리에 존재했으며, 시작 시 `/tmp/memnest-upgrade/before/uncommitted.patch`로 별도 보존했다.
