# MemoryBench 연동

[English](README.md) | [한국어](README.ko.md)

이 폴더에는 MemoryBench의 현재 `Provider` 인터페이스에 맞춘, SDK 없는 `MemnestProvider`와 작은 한국어 코딩 기억 벤치마크 자료가 있습니다. 연동은 다음에 고정되어 있습니다.

- MemoryBench 커밋 `94e2af54b661d90e77dddbd8fa4fa5b28c07a24e`
- MemoryBench 패키지 버전 `1.0.0`
- Bun `1.3.14`(이 연동을 검증할 때 쓴 버전)
- 이 저장소의 Memnest HTTP 주소 `/health`, `/add`, `/search`, `/prune`

비교 점수나 답변 품질 점수는 저장소에 없습니다. 로컬 제공자 자체에는 유료 키가 필요 없으며, 저장소에 있는 검색 전용 점검 결과는 아래에 따로 표시했습니다.

## 약속 검증 (키와 서비스 없이)

Memnest 저장소 루트에서:

```sh
bun test benchmarks/memorybench/test
```

테스트는 프로세스 안의 가짜 HTTP 통신을 씁니다. 요청/응답 대응, 인증 동작, 동기 색인 진행, 정리, 한국어 텍스트 보존, MemoryBench 벤치마크 보기, 자료 참조와 정답을 확인합니다.

## 고정된 MemoryBench로 재현

검토한 원본 리비전을 정확히 받고, 잠긴 의존성을 설치합니다.

```sh
git clone https://github.com/supermemoryai/memorybench.git /tmp/memorybench
cd /tmp/memorybench
git checkout 94e2af54b661d90e77dddbd8fa4fa5b28c07a24e
bun install --frozen-lockfile
node /path/to/memnest/benchmarks/memorybench/install-into-memorybench.mjs /tmp/memorybench
bunx tsc --noEmit
bun test
```

설치 스크립트는 `memnest`를 복사하고 등록하고, `MEMNEST_URL`/`MEMNEST_TOKEN`을 읽고, 원본의 제공자 이름 목록을 늘리기만 합니다. 고정 커밋이 아닌 체크아웃은 거부하고, 검토한 연동 지점이 바뀌었으면 실패합니다.

다른 터미널에서 격리된 로컬 Memnest 서비스를 시작합니다(첫 실행 때 무료 로컬 임베딩 모델을 내려받을 수 있습니다).

```sh
cd /path/to/memnest
rm -rf /tmp/memnest-memorybench-data
cargo run --manifest-path core/Cargo.toml --release -- \
  --data-dir /tmp/memnest-memorybench-data --host 127.0.0.1 --port 3111
```

그다음 `/tmp/memorybench`에서 제공자를 설정합니다.

```sh
export MEMNEST_URL=http://127.0.0.1:3111
export MEMNEST_TOKEN=none
```

서비스 인증을 켰다면 `MEMNEST_TOKEN=<서비스 토큰>`을 쓰세요. 이제 MemoryBench CLI에서 기본 제공자를 고르듯 `memnest`를 고를 수 있습니다. 컨테이너 태그는 격리된 Memnest 프로젝트가 되고, `clear()`는 `/prune`으로 그 프로젝트의 고정되지 않은 벤치마크 기억을 지웁니다.

## 한국어 코딩 기억 자료

`fixtures/korean-coding-memory.json`에는 한국어 코딩 세션 3개와, 사실 회상과 바뀐 포트 결정을 묻는 질문 3개가 있습니다. `KoreanCodingBenchmark`는 현재 MemoryBench `Benchmark` 인터페이스를 구현합니다. 결정적인 테스트로 구조, ID, 질문 거르기, 근거 연결, 필요한 정답 단어가 지정 근거와 정답 양쪽에 있는지를 확인합니다. 이 확인을 답변 점수로 바꾸지는 **않습니다**.

답변 모델이나 채점 모델 없이 일회용 Memnest 서비스에 대고 자료를 돌립니다.

```sh
cd /path/to/memnest
DATA=$(mktemp -d)
core/target/release/memnest --data-dir "$DATA" --port 3112 &
MEMNEST_PID=$!
MEMNEST_URL=http://127.0.0.1:3112 \
  bun benchmarks/memorybench/run-korean-retrieval.ts --out /tmp/memnest-ko.json
kill "$MEMNEST_PID"
```

실행기는 Hit@1, Recall@3, MRR@10, 검색 성공률, 검색 지연 p50/p95, 입력과 색인을 합친 실제 시간, 디스크 바이트, 그리고 `MEMNEST_PID`를 주면 실행 끝의 Linux RSS 한 번을 기록합니다. 답변 정확도와 맥락 토큰은 `null`로 둡니다. MemoryBench 답변 단계 없이 이를 보고하면 검색 근거를 잘못 표시하게 되기 때문입니다. 결과를 기록한 뒤 벤치마크 컬렉션을 지웁니다.

[`results/2026-09-09-local-retrieval.json`](results/2026-09-09-local-retrieval.json)은 Linux x64에서 한 번 돌린 원본 출력입니다. 작은 자료 질문 3개 모두 기대한 세션이 1위였고, 검색 지연 p50/p95는 24.8/27.4 ms, 입력과 색인을 합친 시간은 31.6초, 실행 끝 서버 RSS는 1.71 GB, 색인 파일은 22 KB였습니다. 이것은 기본 점검일 뿐, 비교 결과나 LongMemEval/LoCoMo 정확도 주장이 아닙니다.

이 자료를 MemoryBench 전체 실행에 쓰려면, `korean-coding-benchmark.ts`와 자료를 고정된 체크아웃의 벤치마크 모듈로 복사하고 MemoryBench `src/benchmarks/README.md` 안내대로 등록하세요. 원본의 닫힌 `BenchmarkName` 목록은 Memnest 제공자 연동과 별개라서, 이 저장소는 원본 CLI에 자료를 직접 붙이지 않습니다.

## 공정한 비교 명령과 막힌 점

어댑터를 설치한 뒤 호스팅 Supermemory와 비교하려면, 두 제공자가 같은 자료 구간, 답변 모델, 채점 모델, 처리 설정을 쓰도록 MemoryBench 프로세스 하나로 돌립니다.

```sh
bun run src/index.ts compare -p memnest,supermemory,filesystem,rag -b locomo \
  -m gpt-4o -j gpt-4o
bun run src/index.ts compare -p memnest,supermemory,filesystem,rag -b longmemeval \
  -m gpt-4o -j gpt-4o
```

시드 없는 무작위 표본 대신 전체 자료를 쓰고, 만들어진 compare와 run ID를 기록하고, `data/runs/<run-id>/report.json`, 체크포인트, 질문별 검색 결과를 남기세요. 모델 별칭, 제한, temperature가 다른 별도 실행끼리 비교하지 마세요. 고정된 MemoryBench 모델 목록이 답변과 채점 설정을 정하며, 한 번의 `compare` 실행 안의 모든 제공자가 같은 설정을 씁니다.

- 어댑터와 자료 약속 테스트는 API 키가 필요 없고 유료 API를 부르지 않습니다.
- MemoryBench 전체 답변/평가 실행에는 답변 모델과 채점 모델이 받는 키(`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`)가 하나 이상 필요합니다. 지금 환경에는 승인된 키나 비용 허가가 없어서 위 명령은 돌리지 않았고, 공식 전체 점수는 없습니다.
- 호스팅 Supermemory에는 `SUPERMEMORY_API_KEY`도 필요합니다. `filesystem` 기준선은 OpenAI 추출 호출을, `rag`는 OpenAI 추출과 임베딩을 쓰므로 외부 비용이 듭니다. 고정된 원본의 `supermemory` 제공자는 호스팅 전용이고 같은 기능의 로컬/자체 호스팅 어댑터가 없어서, 다른 제품과 인터페이스를 정의하지 않고는 이 리비전으로 "Supermemory 로컬" 행을 만들 수 없습니다.
- 원본 벤치마크 자료는 GitHub나 Hugging Face에서 받을 수 있어서 네트워크가 필요합니다.
- Memnest는 처음 시작할 때 로컬 임베딩 모델을 받으려고 네트워크가 필요할 수 있고, 이후에는 로컬 캐시를 씁니다.
- 검색 순위는 시험하는 Memnest 실행 파일, 모델, 설정에 따라 다릅니다. 결과를 낼 때는 Memnest 커밋, 모델, MemoryBench run ID, 고른 질문, 모든 CLI 옵션을 기록하세요.
