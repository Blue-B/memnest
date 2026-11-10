# 검색 벤치마크 (memnest 0.5.1)

이 페이지의 숫자는 **검색만** 측정한 결과다. `/search`의 앞쪽 결과에 근거가 들어 있는 기억이 돌아오는지를 본다.
측정, 저장, 검색 어디에도 언어 모델(LLM)을 쓰지 않았다. 숫자는 2026-11-10에 한 대의 컴퓨터에서 배포된
0.5.1 실행 파일로 측정했고, 맨 아래 명령으로 다시 만들 수 있다. 질문별 결과는
[`benchmarks/retrieval/results/`](../benchmarks/retrieval/results/)에 있어 누구나 검증할 수 있다.
질문을 놓치는 이유와 작은 개선안은 [`benchmarks/retrieval/ANALYSIS.md`](../benchmarks/retrieval/ANALYSIS.md)(영어)에 있다.

## 측정한 것과 측정하지 않은 것

측정한 것: Recall@k(앞 k개 결과에 근거 턴/세션이 하나라도 있는 비율), 근거 전부 회수율(모든 근거 턴이 있는 비율),
MRR@10, NDCG@10, 저장 속도, 검색 지연 시간.

**측정하지 않은 것: 질문 답변의 정확도.** LoCoMo와 LongMemEval은 보통 LLM이 답한 뒤 다른 LLM이 채점한 정답률로
발표된다. 그 값은 답변 모델, 프롬프트, 채점 모델에 좌우된다. 검색 회수율은 첫 단계일 뿐이다. 근거 턴이 돌아오지 않아도
답할 수 있고, 돌아와도 틀릴 수 있다. **이 숫자는 다른 도구가 같은 데이터셋에서 발표한 정답률과 비교할 수 없으며**,
"다른 도구보다 낫다/못하다"는 결론도 나오지 않는다.

## 설정

| 항목 | 값 |
|---|---|
| 실행 파일 | 배포된 `memnest 0.5.1`(`/health` 버전 `0.5.1`, sha256 `39a420937cd9c41d...`), 임베딩 모델 `intfloat/multilingual-e5-base`, 로컬 CPU 추론 |
| 인스턴스 | 실행마다 새 임시 데이터 폴더로 띄운 임시 인스턴스, 루프백 포트 3981-3989, `MEMNEST_*` 환경변수 없음, 끝나면 삭제. 모델 캐시는 `~/.memnest/models`에서 하드링크로 가져오며 `~/.memnest`는 수정하지 않는다 |
| 저장 방식 | **대화 턴 하나가 기억 하나.** 대화 기록 감시기(watcher)가 보내는 메타데이터(`chunk_type=auto_log`, `importance=log`, `source=*.transcript`, 세션 ID, 이벤트 ID)와 함께 `POST /add`로 저장. 본문은 LoCoMo는 `[<세션 날짜>] <화자> said: <턴 내용>`, LongMemEval은 감시기와 같은 `User said:` / `Assistant answered:` 접두어. 실제 감시기는 본문에 날짜를 넣지 않지만, 재생한 턴에는 날짜 정보가 전혀 없으므로 여기서는 넣었다(영향은 ANALYSIS.md 4.5절: R@10 약 +1.5점) |
| 범위 | LoCoMo는 대화마다, LongMemEval은 질문 하나의 haystack마다 프로젝트를 따로 만든다. 질문은 자기 프로젝트만 검색한다 |
| 검색 요청 | `POST /search {"query": "<질문 원문 그대로>", "project": "<해당 프로젝트>", "n_results": 10}`. 나머지는 서버 기본값(BM25 + 벡터 하이브리드, RRF 결합, MMR `lambda=0.5`, 거리 한계 0.7, 벡터 전용 보충 3자리). 질문당 요청 한 번이며 Recall@1/3/5는 같은 10개 결과에서 계산한다 |
| `n_results=10`인 이유 | 제품 기본값은 `n_results=3`(HTTP, MCP)이다. Recall@10에는 결과 10개가 필요하다. 10개 요청의 앞 3개는 `n_results=3` 요청과 0.3점 이내로 같다(ANALYSIS.md 4.4절). 따라서 표는 10개를 요청하는 에이전트가 받는 결과이고, R@3이 기본값이 돌려주는 수준이다 |
| LongMemEval 세션 | 돌려받은 턴 중 하나라도 그 세션의 턴이면 세션을 찾은 것으로 센다(그래서 최대 10개 세션만 보인다) |

## LoCoMo (전체: 대화 10개, 턴 5,882개)

`snap-research/locomo`의 `locomo10.json`(커밋 3eb6f2c, sha256 `79fa87e9...`). 질문 1,986개 중 근거가 없는 4개는 제외했고,
대화에 없는 근거 ID 2개는 버렸으며, `D9:1 D4:4` 같은 잘못된 표기는 바로잡았다. 분류는 single-hop, multi-hop, temporal,
open-domain이고, 다섯 번째(adversarial)는 화자가 말하지 않은 내용을 묻는 질문이라 따로 표시한다. 숫자는 %이고
"R@k"는 앞 k개에 근거 턴이 하나라도 있는 비율, "근거 전부 R@10"은 모든 근거 턴이 앞 10개에 있는 비율이다.

| 분류 | 질문 수 | R@1 | R@3 | R@5 | R@10 | 근거 전부 R@10 | MRR@10 |
|---|---|---|---|---|---|---|---|
| **1~4번 분류** | 1536 | 39.7 | 55.4 | 60.7 | 69.1 | 56.4 | 0.490 |
| single-hop | 841 | 43.3 | 60.0 | 65.8 | 72.9 | 70.2 | 0.529 |
| multi-hop | 282 | 25.2 | 41.5 | 46.8 | 60.3 | 14.5 | 0.355 |
| temporal | 321 | 50.2 | 62.3 | 67.3 | 74.5 | 67.6 | 0.576 |
| open-domain | 92 | 15.2 | 31.5 | 34.8 | 43.5 | 20.7 | 0.238 |
| adversarial(5번) | 446 | 31.2 | 46.9 | 52.7 | 58.3 | 57.4 | 0.404 |
| 5개 분류 전체 | 1982 | 37.8 | 53.5 | 58.9 | 66.7 | 56.7 | 0.470 |

1~4번 분류 R@10의 대략적인 95% 구간은 69.1(66.7~71.4), R@1은 39.7(37.3~42.2)이다.
multi-hop 질문은 근거 턴이 여러 개라서 근거 전부 회수율(14.5%)이 하나라도 회수율(60.3%)보다 훨씬 낮다.

## LongMemEval-S (**표본: 비-abstention 질문 470개 중 100개, 전체가 아님**)

데이터셋은 `longmemeval_s_cleaned.json`(LongMemEval README가 현재 가리키는 파일, Hugging Face
`xiaowu0162/longmemeval-cleaned` 리비전 98d7416, MIT 라이선스, sha256 `d6f21ea9...`)이다. 질문마다 세션 약 50개,
턴 400~600개짜리 haystack이 따로 있다.

**고정 시드로 층화 추출한 표본이며 전체 벤치마크가 아니다.** 비-abstention 질문 470개에서 질문 유형 비율에 맞춰
100개를 뽑았다(시드 1110, ID 목록은 결과 파일에 있음). 전체는 기억 약 246,600개(턴 평균 약 990자, LoCoMo의 6배)라
시간 예산에 들어가지 않았다. 이 표본의 기억 49,117개를 저장하는 데 임시 인스턴스 두 개를 병렬로 돌려 3.5시간이 걸렸다.
전체의 abstention 질문 30개는 설계상 검색 평가에서 제외하며 표본에도 없다. 유형별 질문이 6~27개뿐이라 유형별 행은
거칠다. 질문 하나가 4~17점을 움직인다. 전체 수치의 대략적인 95% 구간은 세션 R@1이 83.0(74.5~89.1),
세션 R@10이 94.0(87.5~97.2)이다.

| 질문 유형 | 질문 수 | 세션 R@1 | R@3 | R@5 | R@10 | NDCG@10 | 턴 R@10(하나라도) | 턴 R@10(전부) |
|---|---|---|---|---|---|---|---|---|
| **전체(abstention 제외)** | 100 | 83.0 | 89.0 | 91.0 | 94.0 | 0.837 | 79.0 | 54.0 |
| knowledge-update | 15 | 100.0 | 100.0 | 100.0 | 100.0 | 0.956 | 93.3 | 80.0 |
| multi-session | 26 | 80.8 | 96.2 | 100.0 | 100.0 | 0.841 | 84.6 | 30.8 |
| single-session-assistant | 12 | 100.0 | 100.0 | 100.0 | 100.0 | 1.000 | 75.0 | 75.0 |
| single-session-preference | 6 | 50.0 | 50.0 | 50.0 | 66.7 | 0.559 | 50.0 | 33.3 |
| single-session-user | 14 | 100.0 | 100.0 | 100.0 | 100.0 | 1.000 | 92.9 | 92.9 |
| temporal-reasoning | 27 | 66.7 | 74.1 | 77.8 | 85.2 | 0.671 | 66.7 | 37.0 |

| 질문 유형 | 질문 수 | 정답 세션 전부 R@5 | R@10 |
|---|---|---|---|
| **전체(abstention 제외)** | 100 | 73.0 | 78.0 |
| knowledge-update | 15 | 86.7 | 93.3 |
| multi-session | 26 | 57.7 | 69.2 |
| single-session-assistant | 12 | 100.0 | 100.0 |
| single-session-preference | 6 | 50.0 | 66.7 |
| single-session-user | 14 | 100.0 | 100.0 |
| temporal-reasoning | 27 | 59.3 | 59.3 |

"세션 R@k": 앞 k개 턴이 속한 세션 중 정답 세션이 하나라도 있는 비율. "정답 세션 전부"는 모든 정답 세션이 있어야 한다(multi-session은 최대 6개).
"턴 R@10": 데이터셋이 `has_answer`로 표시한 턴 기준이며 "하나라도"/"전부"로 나눈다.
temporal-reasoning 질문은 데이터셋의 `question_date` 없이 원문 그대로 보냈기 때문에("10일 전에 산 주방 기기" 같은 상대 시간을
검색이 해석할 수 없다) 가장 약한 유형이다.

## 속도 (한 대의 컴퓨터에서 측정)

| 항목 | 값 |
|---|---|
| 저장 속도, LoCoMo(짧은 턴, 평균 164자), 인스턴스 1개, `POST /add` 순차 | **초당 6.7개**(5,882개를 881초에) |
| 검색 지연 시간, LoCoMo, 순차 요청, `n_results=10`, 1,986건 | **p50 38.6ms, p95 47.6ms**(클라이언트에서 잰 전체 시간, 질문 임베딩 포함. 서버 보고값 p50 37ms, p95 46ms) |
| 측정 중 컴퓨터 부하 | 1분 평균 부하 시작 시 1.6, 평균 1.9(논리 CPU 10개) |
| LongMemEval 저장 속도 | 한가한 컴퓨터에서 재지 않았다. 턴이 약 6배 길고, 표본은 다른 작업이 돌던 컴퓨터에서 인스턴스 2개를 병렬로 돌렸다(인스턴스당 초당 약 2개). 그래서 수치를 공개하지 않는다 |

위 시간은 한가한 컴퓨터에서 한 번 순차로 돌린 결과다. 같은 데이터를 다른 작업이 도는 중에 잰 이전 측정(초당 5.4개, p95 68ms)은
부하의 영향을 받아 쓰지 않는다. 지연 시간은 검색 대상 프로젝트의 기억 수(프로젝트 안 벡터를 전부 비교)에 따라 늘어난다.
여기서는 LoCoMo 369~689개, LongMemEval 409~606개였다.

하드웨어: AMD Ryzen 5 9600X(6코어, WSL2에서 보이는 논리 CPU 10개), RAM 23.5GiB, WSL2 위의 Linux 6.18, **GPU 없음**(CPU 추론).

## 데이터셋, 라이선스, 출처

데이터셋은 `~/.cache/memnest-bench`에 내려받으며 저장소에는 넣지 않는다. 결과 파일에 원본 URL, 리비전, 라이선스, sha256이 기록된다.

* LoCoMo: <https://github.com/snap-research/locomo>, CC BY-NC 4.0(비영리. 여기서는 검색 측정에만 쓰고 재배포하지 않음).
* LongMemEval: <https://github.com/xiaowu0162/LongMemEval>, 정리본 <https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned>, MIT.

## 한계

* 질문 답변 정확도가 아니라 검색 회수율이다. 데이터셋이 모두 영어이므로 한국어나 혼용 문장의 검색 성능은 이 페이지로 알 수 없다.
* LongMemEval은 100문항 표본이다. LoCoMo는 대화마다 화자가 둘로 고정되어 있고 근거 표시에 잡음이 있다(이미지뿐인 턴이거나 실제 답 바로 옆 턴인 경우).
  LoCoMo가 제공하는 이미지 캡션은 저장하지 않았다.
* 하네스는 턴을 하나씩 저장한다. 실제 사용에는 이 대화에 없는 도구 출력, 코드 같은 잡음이 섞인다.
* 한 대의 컴퓨터에서 설정당 한 번 돌렸다. 같은 LoCoMo 데이터를 두 번 돌렸을 때 1,986개 질문 중 9개에서 상위 10개 순서가 달랐다(모든 지표 0.1점 이내).
  따라서 빌드 사이의 아주 작은 차이는 의미가 없다.
* LoCoMo 질문 1,986개 중 330개(17%, 아포스트로피 `Caroline's`나 콜론이 들어간 질문)는 0.5.1에서 키워드(BM25) 검색이 빠지고 결과가 3개만 돌아온다.
  ANALYSIS.md 4.1절 참고. 위 표에는 이 영향이 포함되어 있다. 즉 현재 0.5.1의 실제 동작이다.

## 다시 만들기

```sh
python3 benchmarks/retrieval/test_metrics.py                                   # 지표 함수 자체 점검
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset locomo                          # 약 25분
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset longmemeval --sample 100 --seed 1110   # 이 페이지의 표본, 약 3.5시간
# LongMemEval-S 전체(기억 약 246,600개, 표본에서 추정한 약 17시간이며 직접 재지 않음). 인스턴스를 나눠 돌릴 수 있다:
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --shard 0/2 --port 3981 --checkpoint /tmp/lme0.jsonl --out-dir /tmp/lme-shards --tag shard0 &
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --shard 1/2 --port 3982 --checkpoint /tmp/lme1.jsonl --out-dir /tmp/lme-shards --tag shard1 &
python3 benchmarks/retrieval/merge.py --out benchmarks/retrieval/results/<날짜>-v<버전>-longmemeval.json /tmp/lme-shards/*.json
```

다른 빌드를 시험하려면 실행 파일 경로만 바꾸면 된다. 결과 파일 이름에는 `/health`가 보고한 버전이 들어간다.
병렬 인스턴스에는 각자 `--port`(3981-3989)를 주고, `~/.memnest/models`에 임베딩 모델이 있어야 한다(임시 데이터 폴더로 하드링크됨).
시간 측정은 다른 작업이 없는 컴퓨터에서 하고 `uptime`을 확인한다. 결과 파일에 측정 당시 부하가 기록된다.
