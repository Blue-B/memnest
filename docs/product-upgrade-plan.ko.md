# 제품 개선 계획: 신뢰할 수 있는 기억부터

<!-- markdownlint-disable MD013 -->

## 범위와 출시 상태

이 문서는 작업 시작 HEAD `6b2da2804d0806fe9495b9039ea8303cb428a0b0` 위의 기존 미커밋 변경을 보존하면서 진행한 두 개선 조각(기억 무결성, 검색 평가/읽기 전용 진단)을 기록합니다. 공개 코어 `0.2.1` / npm `0.2.0`에 이 변경이 배포되었다는 뜻이 아닙니다. 버전·서비스·클라이언트 설정·운영 DB는 변경하지 않았습니다.

목표는 저장 → 정정 → 조회 → 삭제 → 재시작 이후에도 사용자가 의도한 기억만 찾게 하는 것입니다. 새 생성 모델, 검색 엔진 재작성, UI 확장은 범위 밖입니다. Supermemory보다 낫다는 주장은 하지 않습니다.

## 우선순위와 수락 기준

| 우선순위 | 개선 | 명시적 수락 기준 | 이번 상태 |
| --- | --- | --- | --- |
| P0 | 삭제한 대화 재수집 방지 | 휴지통 복구 기간에는 복구 가능. 영구삭제 뒤 동일 이벤트 재수집은 본문을 만들지 않으며, 일반 재시작과 강제 재색인 후에도 동일. 실패 시 삭제와 재수집 방지 표식은 함께 롤백 | 재현 후 수정·검증 |
| P0 | 정정한 사실의 현재 버전 검색 | 같은 프로젝트의 유효한 대상을 `supersedes`로 정정하면 새 기록만 검색. 없는 대상·다른 프로젝트·이미 대체된 대상은 기존 사실을 숨기거나 새 사실을 저장하지 않음 | 기존 동작 검증. 오류 응답 개선은 후속 |
| P0 | 수정 직후 검색/범위 일치 | 본문·프로젝트 수정 후 새 단어는 새 범위에서 검색되고 옛 단어/옛 범위에서는 나오지 않음. 재시작·재색인 후 동일 | HTTP/MCP 실동작 검증 |
| P1 | 처음 설치부터 첫 기억까지 | 격리된 설치에서 저장→검색→ID 조회→수정→삭제를 재현. 준비 실패 시 저장 요청을 보내지 않음. 기본 자동 회상은 꺼짐. 설정 변경 전 백업과 되돌리기 안내 | 다음 조각. 기존 설치 검사를 재사용하되 실제 서비스/설정은 건드리지 않음 |
| P1 | 읽기 전용 진단 | 모델 준비 상태·색인 대기/실패·수집 heartbeat를 원문/비밀 없이 구분. 정상 health가 곧 검색 준비 완료라는 오해 방지 | source `status --diagnose` 구현; 공개되지 않은 준비/색인 상태는 unknown |
| P2 | 검색 품질과 규모 | 한국어/영어, 부정 질문, 정정 이력, 동일 basename 작업공간을 포함한 고정 정답셋. 정확도·누락률·지연·RSS를 함께 측정하고 실행 환경/원시 결과 공개 | 고정 26질문/16기록 소규모 평가 완료; 대규모/동일 basename 평가는 후속 |

## 확인된 결함과 수정

### 영구삭제된 transcript가 backfill로 살아남

기존 흐름은 `watch::post_event` → HTTP `/add` → 공통 `operations::remember` → 결정적 transcript ID 조회였습니다. 휴지통에 행이 남아 있는 동안은 중복으로 처리하지만, `lifecycle::prune_trash`가 30일 뒤 `Database::delete_chunk`로 행을 없애면 같은 이벤트가 신규 저장으로 처리되었습니다.

먼저 `purged_transcript_replay_does_not_resurrect` 회귀를 추가했습니다. 수정 전에는 재전송 응답이 `deduplicated`가 아닌 `succeeded`라 실패했고, 삭제한 행이 재생성되었습니다.

수정 내용:

- `transcript_tombstones(id)`를 기존 `CREATE TABLE IF NOT EXISTS` 초기화 패턴으로 추가했습니다. 본문·프로젝트·경로·시간은 저장하지 않습니다.
- 영구삭제·ID 표식 생성·색인 삭제 큐 기록이 같은 SQLite transaction에서 실행됩니다. 현재 소스의 영구삭제 경로는 `prune_trash` → `delete_chunk` 하나입니다.
- API는 이미 영구삭제된 이벤트에 기존 watcher 계약인 `deduplicated`를 반환합니다. 이는 살아 있는 기억을 반환한다는 뜻이 아니라 해당 이벤트를 다시 처리할 필요가 없다는 뜻입니다. 이 ID의 직접 조회는 404입니다.
- 임베딩 전에 확인만 하면 삭제와 경합할 수 있으므로, 실제 transcript 저장 시에도 transaction 안에서 표식 및 기존 행을 확인합니다. 재전송은 휴지통/대체 기록이나 사용자가 수정한 기존 본문을 덮어쓰지 않습니다.
- 일반 수동 기억의 UUID에는 표식을 만들지 않습니다. 같은 내용을 사용자가 수동으로 새로 저장하는 것은 막지 않습니다. 휴지통에서 명시적으로 복구하는 기존 동작도 유지합니다.

### 보존과 삭제의 한계

불투명한 결정적 이벤트 ID는 **무기한 보관**합니다. 표식 삭제 UI/설정은 이번에 추가하지 않습니다. 이미 이전 버전에서 영구삭제된 기록에는 ID 표식이 없으므로 소급 방지할 수 없습니다. 원본 transcript 파일은 외부 호스트에 그대로 남습니다. 파일 이동·교체, 프로젝트/수집 이벤트 ID 변경 등으로 다른 결정적 ID가 만들어지면 다른 이벤트로 취급합니다. 이것은 내용 기반 영구 차단이나 보안 삭제가 아닙니다.

기존 GC의 `archive/YYYY-MM.jsonl` 본문 보관 정책도 바꾸지 않았습니다 (`MEMNEST_ARCHIVE=0`으로 비활성화 가능). 이번 실동작 검사는 임시 데이터에서 archive를 껐습니다. 표식 자체에는 원문이 없지만, 원문이 어디에도 남지 않는다고 주장하지 않습니다.

## 이미 확인한 불변 조건

- HTTP와 MCP의 공통 기억 연산은 그대로 유지했습니다. watcher도 기존 HTTP 저장 경로를 사용합니다.
- `_trash`, `_superseded`는 검색에서 숨겨지고, SQLite가 색인 재구축의 기준입니다. 숨겨진 행의 ID 직접 조회는 기존 계약대로 가능합니다.
- 검색은 결과당 600자 발췌, 전체 검색 글자 수 제한 없음. `get` 기본 8,000자 및 페이지 이어 읽기를 변경하지 않았습니다.
- 자동 회상 기본 꺼짐, 선택적 회상 설정, source-only/배포판 구분을 유지했습니다.

## 재현 가능한 검증

기존 `core/target/test-model-cache`가 필요합니다. 모델을 받는 설치 명령이 아닙니다.

```sh
cd core
HF_HUB_OFFLINE=1 HTTP_PROXY=http://127.0.0.1:1 HTTPS_PROXY=http://127.0.0.1:1 \
  ALL_PROXY=http://127.0.0.1:1 NO_PROXY=127.0.0.1,localhost \
  cargo test --offline --lib -- --test-threads=1
cargo build --offline
cargo fmt --check
cd ..
python3 scripts/test-memory-lifecycle.py
ruff check scripts/test-memory-lifecycle.py
```

- Rust lib 113개 및 binary 4개, 총 117개 통과. 새 회귀 3개: 실제 GC 뒤 재전송, 표식 transaction 실패/재개방/수동 저장 독립성, 동시 삭제/재전송 10회.
- 새 Python 검사는 임시 DB·랜덤 loopback 포트·기존 캐시만 사용합니다. 실제 HTTP/MCP 저장/정정/수정/삭제, 실제 watcher 재수집, 실제 120초 후 예약 GC, 재시작/강제 재색인을 확인했습니다. 테스트가 시작한 자식 프로세스만 종료합니다.
- 이 검사는 수집/검색 가시성 계약의 증거이지 모든 동시 요청의 직렬화나 답변 정확도의 증명은 아닙니다. 단어/프로젝트 수정과 같은 ID를 동시에 다루는 일반 update의 lost-update 가능성은 별도 검토 대상입니다.

## 지금 후보로 낼 것 / 나중에 할 것

**검토 후 출시 후보:** 이번 표식 수정과 회귀 검사, 기존 원문 페이지 읽기/기본-off 회상의 별도 승인된 변경. 실제 배포/버전 갱신은 아직 하지 않습니다.

**다음 조각:** 잘못된 `supersedes`가 현재 일반 HTTP 500으로 응답하는 문제를 사용자 오류 4xx와 구체적 메시지로 분리하기. 현재 검사는 실패 시 기존 행이 보존됨을 확인하지만 오류의 사용성은 충분하지 않습니다. 읽기 전용 진단과 처음 사용 흐름을 이어서 개선합니다. 기존 `preflight.sh`는 고정 포트/공유 임시 경로 및 준비 실패 후 요청 가능성을 먼저 고친 다음 사용할 대상이지 이번 검증 명령이 아닙니다. CONTRIBUTING의 미게시 관련 설명도 출시 상태와 대조가 필요합니다.

**별도 검증 환경 필요:** native macOS 설치/launchd, 대규모 부하 및 유료 경쟁 제품 비교. 기존 한국어 3개 쿼리 p50 약 25ms/RSS 약 1.71GB는 제한된 선행 관찰일 뿐 비교 우위나 대표 품질 수치가 아닙니다.

## 두 번째 조각: 재현 가능한 검색 평가와 첫 진단

### 고정 정답셋과 결과 (소스 빌드, 배포 아님)

- `scripts/fixtures/coding-memory.json`: 사람이 먼저 지정한 16개 기록/26개 질문. 영어·한국어 변형, 파일명, 폐기/현재 포트, 방해 기록, 프로젝트 분리, 정답 없는 4개 질문. 쿼리 실행 전에 정답을 고정했으며 결과에 맞춰 바꾸지 않았습니다.
- `scripts/evaluate-coding-memory.py`: 실제 HTTP `/add` + `/search`, 같은 프로젝트의 `supersedes`, 임시 DB와 랜덤 loopback 포트. source CLI의 인증 성공/실패도 실제 서비스에서 확인합니다. 검색 반환 ID가 허용된 프로젝트의 현재 기록인지 검사합니다. 품질 점수 자체는 통과 기준으로 둔갑시키지 않습니다.
- 단순 기준선: 본문/파일명을 Unicode `\w+` 소문자 토큰으로 나눈 교집합 개수, 동률은 고정 기록 순서, 겹침 0은 반환하지 않음. 같은 범위/현재성 필터, 최대 5개. 형태소 분석·불용어 제거·학습/튜닝 없는 약한 기준선이며 유료 제품 비교가 아닙니다.
- 환경: Linux WSL2 x86_64, debug core 0.2.1 소스, 기존 multilingual-e5-base 캐시, 외부 다운로드 차단, 순서가 고정된 한 번의 warm 쿼리 순회. 별도의 확인 실행에서도 순위 집계는 같았습니다. 새 생성 모델/LLM 심판 없음.

| 측정 (정답 있는 22개 / 없는 4개) | 실제 서비스 | 단순 토큰/파일 기준선 |
| --- | --- | --- |
| Hit@1 | 18/22 (81.82%) | 17/22 (77.27%) |
| Recall@5 (각 양성 질문의 정답은 1개) | 20/22 (90.91%) | 17/22 (77.27%) |
| MRR@5 (누락=0) | 0.8636 | 0.7727 |
| 정답 없는 질문에 빈 목록 | 1/4 (25%) | 3/4 (75%) |

기록 저장 후 서비스 쿼리 지연 p50 29.101ms / p95 31.688ms (정렬 후 index floor(n·p), 네트워크 왕복 포함), 쿼리 후 VmRSS 1,734,620 kB. 콜드 시작·큰 자료·동시 부하 수치가 아닙니다. [질문별 반환 키·점수·시간 및 집계 원시 JSON](evaluations/coding-memory-source.json)을 함께 보관합니다. raw JSON의 임시 endpoint는 재사용할 설치 주소가 아닙니다.

### 실패를 숨기지 않기

- `port-ko`, `tests-ko`: 한국어 배포 기록이 먼저 나오고 실제 정답은 2위입니다.
- `deploy-en`, `timezone-en`: 영어 질문에 한국어 정답이 top5에 없습니다. 저장 누락이 아님은 해당 한국어 질문에서 같은 ID가 나오는 것으로 확인했습니다.
- `absent-en`, `absent-ko`, `absent-domain`: 정답이 없어도 각각 5/3/3개의 관련 없는 결과가 나옵니다. 빈 프로젝트만 확실히 빈 목록입니다. 검색 결과가 있다고 사실이 존재한다고 가정하면 안 됩니다.
- 코드 추적: `run_hybrid_search_scope`는 lexical 결과가 있으면 vector-only 예산을 0으로 만들고, text hit 또는 keyword match는 semantic cutoff 없이 통과시킵니다. 이는 다국어 누락/일반 단어의 false positive와 부합하는 기존 정책입니다. 이 작은 정답셋만으로 전역 cutoff/랭킹을 임의 조정하지 않았고, 원인이 단일 버그라고 단정하지 않습니다. 독립된 더 큰 holdout 및 후보별 추적 후 정책 변경을 승인받는 것이 다음 단계입니다.
- 프로젝트/현재성 위반은 26개 모두 0건. 이 평가만으로 일반적 검색 우위, 생성 답변 정확도, 모든 동시성 또는 내용 삭제를 증명하지 않습니다.

### 읽기 전용 진단과 운영 안내

`memnest --host 127.0.0.1 --port <실제 포트> status --diagnose`는 기존 health와 MCP tools/list만 사용합니다. HTTP 전체 5초, 응답당 256KiB 제한, redirect/proxy 차단, 인증 오류/연결 실패/구버전 schema를 구분합니다. 토큰은 기존 `MEMNEST_TOKEN` 환경변수에서만 받고 출력하지 않습니다. 도구 **실행**/DB 열기/모델 초기화는 없습니다. 광고된 source search/get 필드만 확인하며 전체 클라이언트 호환성이나 준비 완료를 보증하지 않습니다.

현재 health에는 모델 준비·색인 대기/실패·원격 수집 heartbeat가 없으므로 unknown으로 표시합니다. 기존 `status`는 지정한 watcher data-dir의 로컬 heartbeat/last stored를 읽습니다. 진단 경로는 시간/읽기 범위를 제한하기 위해 그 파일도 읽지 않습니다. 기존 `--doctor`는 write-test와 index open을 하므로 읽기 전용이라고 안내하지 않습니다. [운영 문서](operations.md)에 실제 데이터와 임시 데모, source/배포 구분, 오류별 복구 안내를 추가했고 CONTRIBUTING의 미게시 주장을 바로잡았습니다. 안전하지 않은 legacy `preflight.sh`는 이번 실행에 사용하지 않았고 대체 격리 harness를 안내합니다.

검증: 새 CLI test double 8종(정상/401/구 schema/잘못된 JSON/과대 응답/redirect/timeout/unreachable), metric/fixture 단위 3개, 실제 서비스 인증과 평가, 기존 HTTP/MCP bounded retrieval 모두 통과. Rust lib 113 + binary 4 통과. `cargo fmt --check`, offline build/check all-targets, lib clippy 및 Python ruff 통과. 패키지 자산은 바뀌지 않아 npm 재빌드는 하지 않았습니다. 네이티브 macOS/유료 비교/대규모 평가는 여전히 미실행입니다.

### P1 미완: lexical distractor가 vector-only 정답을 제거하는 정책

후속 독립 재현 `server::api::test_support::lexical_distractor_suppresses_even_exact_vector_candidate`
(`core/src/server/api.rs:2283`)은 위 26사례/모델 품질과 별개입니다. 동일 임시 프로젝트에
한국어 정답 문서 하나를 넣되 임베딩은 질의 벡터와 **의도적으로 동일하게** 지정합니다.
모델이 실제로 이 문서를 완벽하게 임베딩한다는 주장이 아니라 후보 필터만 분리하는 fixture입니다.

1. `scheduler timezone` 질의에 lexical hit/keyword overlap이 없고 정답 vector distance가 cutoff 이내임을 먼저 검사합니다. 정답만 있을 때 반환 ID는 `[semantic-target]`입니다.
2. 같은 프로젝트에 시간대와 무관한 로고 폰트명 `Timezone` 방해 문서를 추가합니다. 방해 벡터는 질의의 반대 방향으로 고정합니다. lexical 결과가 `[lexical-distractor]`만임을 검사합니다.
3. top5에 여유가 있는데 반환은 `[lexical-distractor]`뿐입니다. 정답을 버리는 현재 동작을 특성화하는 재현이며, 바람직한 품질을 통과했다고 표시하는 테스트가 아닙니다.

정확한 seam: `core/src/server/api.rs:446-464`에서 두 후보군을 생성/융합한 **뒤**,
`:468-474`에서 lexical 결과가 하나라도 있으면 vector-only 예산을 0으로 만듭니다.
`:515-525`는 keyword/text hit가 없는 정답을 cutoff 이내라도 버립니다. 따라서 후보 생성에서
정답을 찾지 못한 결함이 아니라, 명시적 보수적 semantic-fallback 정책의 **후보 채택 단계**가
정답을 제거하는 재현입니다. 정책의 존재는 명확하지만 어떤 제품 요구로 선택됐는지는 코드만으로
확정하지 않습니다. 기존 정책 준수와 만족스러운 recall은 다른 문제입니다.

**높은 우선순위 미완(P1):** 무관한 lexical 후보 때문에 좋은 semantic 후보를 전부 버리는
non-monotonic recall을 어떻게 완화할지 독립 holdout/candidate trace로 검토하고 승인받아야 합니다.
이번에 production gate/threshold/ranking은 변경하지 않았습니다. 위 재현은 고정 26사례의 통계에
추가하지 않았고, 원시 JSON/fixture SHA-256도 그대로 유지했습니다.

```bash
cd core
HF_HUB_OFFLINE=1 HTTP_PROXY=http://127.0.0.1:1 HTTPS_PROXY=http://127.0.0.1:1 \
  ALL_PROXY=http://127.0.0.1:1 NO_PROXY=127.0.0.1,localhost \
  cargo test --offline --lib lexical_distractor_suppresses_even_exact_vector_candidate -- --test-threads=1
```

### 검색 점수와 자동 주입 보류는 별도 계약

raw search `score`는 `api.rs:527-535`의 RRF+keyword/importance/type 보너스−recency 조합이지
관련성 확률이 아닙니다. negative 빈 목록 1/4는 **직접 검색**의 abstention 관찰이며 자동 회상
주입률/안전성 측정이 아닙니다. pi 자동 회상은 기본 off (`pi-extension/src/autocontext.ts:24`),
opt-in 이후에도 별도 범위/종류 필터와 MIN_SCORE 및 최대 주입 수 (`:181-188`)가 있습니다.
그 threshold 역시 확률 보증이 아닙니다. 이번 26사례는 자동 주입이나 생성 답변을 실행하지 않았으므로
직접 검색의 false positive가 반드시 주입된다고도, 자동 회상 필터가 안전하게 막는다고도 주장하지
않습니다. explicit search recall 개선과 automatic-injection abstention 정책은 독립적으로 평가해야 합니다.

### 진단 disclosure 보완 검증

HTTP 응답의 `version` 문자열도 신뢰할 수 없으므로 진단에서는 응답 문자열을 출력하지 않도록
보완했습니다. 응답 읽기 실패도 정해진 transport 메시지만 출력합니다. 테스트는 version/도구 설명/비JSON
응답에 토큰·원격 로컬경로·고유 비밀 sentinel을 넣고 stdout/stderr에 없음을 검사합니다. 명령의
`--data-dir`도 출력/생성하지 않습니다. 종료 상태는 machine-readable **0=광고된 계약 확인 성공,
1=실패**이며 401/구 schema/timeout 등을 별도 숫자로 구분하는 JSON API는 추가하지 않았습니다.
상세 원인은 정해진 진단 문장으로 구분합니다. 새 disclosure 8종 재실행, 실제 인증 서비스 roundtrip,
Rust lib 113+binary 4(총 117), fmt/lib clippy/all-target check 통과. 최초 raw 평가 JSON의 diagnostic
텍스트는 당시의 응답 버전 표시를 포함한 실행 기록으로 보존했고 새 결과로 덮어쓰지 않았습니다.

### 부모 최종 검증과 리뷰 지적 처리

독립 리뷰는 lifecycle 검사기가 포트 경쟁 상황에서 다른 서비스의 health 200을 받아
그 서비스에 저장할 수 있다는 P1을 지적했습니다. 부모가 검사기를 수정했습니다.
매 시작과 재시작에서 health의 `data_dir`를 임시 경로와 대조하고, 무작위 전용 토큰을
서버와 watcher에 전달합니다. HTTP는 프록시와 리다이렉트를 거부하며 loopback만 허용합니다.
이 안전 검사는 Python `-O`에서도 제거되지 않습니다.

`python3 scripts/test-lifecycle-safety.py`는 살아 있는 잘못된 서버 응답에 대해 쓰기 없이
실패하는지, 환경변수 프록시를 우회하는지, 인증 실패와 리다이렉트를 거부하는지 검사합니다.
일반 실행과 `-O` 실행 모두 두 테스트가 통과했습니다. 부모가 수정 diff를 검토했으며
수정 후 독립 재리뷰를 받았다는 뜻은 아닙니다.

부모 재실행 결과: Rust 113+4개, 진단 8종, 평가 산식 3개, Python lint,
변경 Rust/Python 6개 파일의 primary LSP, release 빌드와 packaging 검사 통과.
최종 release 바이너리로 실제 HTTP/MCP, watcher, 120초 GC, 재시작과 강제 재색인 경로도
다시 통과했습니다. scoped cached lens에는 차단 진단이 없으며 전체 저장소 검사 결과는 아닙니다.

이번 수정과 진단 기능은 소스에만 있습니다. 운영 설치와 데이터는 바꾸지 않았고 커밋이나
배포도 하지 않았습니다. 검색 후보 채택 정책 P1, 잘못된 정정 대상의 HTTP 500,
모델과 색인의 준비 상태 관측, 실제 사용자 베타와 경쟁 평가는 여전히 남아 있습니다.
따라서 이 검증은 이번 구현 범위의 수락이며 제품 전체나 출시 준비 완료 판정이 아닙니다.
