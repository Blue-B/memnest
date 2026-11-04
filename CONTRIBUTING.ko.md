# memnest 기여 안내

[English](CONTRIBUTING.md) | [한국어](CONTRIBUTING.ko.md)

관심 가져 주셔서 고맙습니다. memnest는 작은 프로젝트라 절차도 짧습니다.

## 시작하기 전에

참여하면 [행동 강령](CODE_OF_CONDUCT.md)(영문)에 동의하는 것으로 봅니다.

버그 수정보다 큰 작업이라면 먼저 이슈를 열고 겪은 문제를 설명해 주세요. 변경을 다 만든 뒤에야 설계와 맞지 않는다는 걸 아는 일을 막을 수 있습니다. 작은 수정은 바로 풀 리퀘스트를 보내도 됩니다.

코어와 pi 확장은 버전을 함께 올립니다. crates.io 배포를 뜻하지는 않습니다. 소스만 있는 macOS 지원은 확인되지 않았고, 1.0 전까지 구조와 인터페이스가 바뀔 수 있습니다.

## 브랜치

`main`에는 정식 출시만 올립니다. 개발 중인 변경은 `develop`에 올리고, 출시할 때 `develop`을 `main`에 합친 뒤 `v*` 태그를 붙입니다. 풀 리퀘스트는 `develop`으로 보내 주세요.

## 저장소 구조

memnest를 실행하는 데 필요한 것은 `core/`뿐입니다. 나머지는 그 주변의 선택 요소입니다.

| 폴더 | 언어 | 내용 |
| --- | --- | --- |
| `core/` | Rust | 엔진: HTTP API, MCP 서버, 색인 |
| `pi-extension/` | TypeScript | pi 연동 |
| `adapters/generic-http/` | JavaScript | 참고용 JSONL 어댑터 |
| `docs/` | Markdown | 운영 안내와 스크린샷 |

## 개발 검사

건드린 부분의 검사를 돌리세요. 각 블록은 하위 셸이라, 앞 줄이 어디서 끝났든 저장소 루트에서 시작합니다.

```bash
(cd core                   && cargo test --locked -- --test-threads=1)
(cd pi-extension           && npm install && npm run build && npm run smoke)
(cd adapters/generic-http  && node test.mjs)
```

코어 테스트는 일부러 순서대로 돌립니다. 여러 테스트가 프로세스 전역 상태(`MEMNEST_*` 환경 변수와 공유 금고 암호기)를 바꾸기 때문에, 기본 병렬 실행에서는 서로 충돌합니다. CI도 같은 `--test-threads=1`을 씁니다.

CI는 테스트 뒤에 릴리스 실행 파일을 빌드하므로 `core`는 `cargo build --release --locked`도 통과해야 합니다. 의존성을 바꿨다면 `python3 scripts/check-licenses.py`와 `python3 scripts/generate-third-party-notices.py --check`도 돌리세요.

`pi-extension`에는 스모크 테스트 외에 `npm run e2e`(`node test/e2e-mcp.mjs`)가 있습니다. 실제 memnest 실행 파일을 stdio로 띄우므로 `MEMNEST_BIN`으로 지정하세요: `MEMNEST_BIN=../core/target/release/memnest npm run e2e`. `dist/index.mjs`는 커밋되어 있고, CI는 새로 `npm run build`한 결과와 다르면 거부합니다. 그래서 `src/`를 바꿨다면 다시 빌드한 번들도 함께 커밋하세요.

### 스모크 테스트는 실행 중인 memnest에 연결합니다

실제로 데이터를 잃은 사람이 있으니 분명히 적어 둡니다.

스모크 테스트는 받은 주소에 응답하는 저장소에 기록합니다. 평소에 쓰는 memnest를 절대 가리키지 마세요. 임시 인스턴스를 쓰세요.

```bash
memnest --data-dir /tmp/memnest-dev --port 3150 &
MEMNEST_URL=http://127.0.0.1:3150 MEMNEST_DB=/tmp/memnest-dev/memory.db npm run smoke
```

`pi-extension` 스모크 테스트는 `http://127.0.0.1:3111`에도 접속하지만, 응답이 없으면 실제 연결 구간을 건너뛰고, 호출도 읽기 경로뿐입니다. 기억을 만들지 않습니다.

## 지속적 통합(CI)

워크플로는 `.github/workflows/`에 있고 경로로 걸러지므로, `core/` 아래 변경은 pi-extension 작업을 돌리지 않습니다.

- `core-ci.yml`: Linux와 Windows에서 순차 테스트와 릴리스 빌드를 돌리고, 설치 스크립트를 실행하고, 라이선스 정보를 확인하고, `THIRD_PARTY_NOTICES.md`가 오래되면 실패합니다.
- `pi-extension-ci.yml`: `npm ci`로 설치하고, 번들을 빌드하고, 커밋된 `dist/index.mjs`를 그 결과와 비교하고, 불러 보고, 새로 빌드한 코어 실행 파일로 MCP 전체 테스트를 돌립니다.
- `adapters-ci.yml`: 일반 HTTP 약속 테스트를 돌립니다.
- `core-release.yml`: `core/Cargo.toml`의 버전과 맞지 않는 태그를 거부하고, 테스트하고, 빌드한 뒤, 무엇이든 배포하기 전에 압축 파일을 풀어 포장된 실행 파일을 돌려 봅니다.

## 커밋 메시지

이력은 Conventional Commits를 따르고, 변경이 한 패키지에 속하면 그 패키지 이름을 범위로 씁니다.

```text
feat(core): serve MCP over streamable HTTP on the API port
fix(pi-extension): make autocontext triggers multilingual
docs: rewrite the README around what a stranger needs
chore: clean up the repository for a public release
```

쓰는 범위는 `core`, `pi-extension`, `adapters`입니다. 저장소 전체에 걸친 `docs`와 `chore` 변경은 범위 없이 씁니다. 본문에는 왜 바꿨는지를 약 80자 폭으로 줄바꿈해 적습니다.

## 풀 리퀘스트

무엇을 왜 바꿨는지, 실제로 어떤 검사를 돌렸는지 적어 주세요. 돌리지 못한 검사가 있다면 암시하지 말고 그렇다고 적어 주세요. 검토자는 추측하는 것보다 "memnest 서버가 없어서 pi-extension 스모크 테스트는 돌리지 못함"을 읽는 편을 원합니다.

풀 리퀘스트 하나에는 한 가지 문제만 담으세요. 관련 없는 수정 두 개는 풀 리퀘스트 두 개로 나누는 편이 검토하기 쉽습니다.

## 버그 신고와 기능 요청

이슈 템플릿을 쓰세요. 버그라면 `memnest --version`과 `memnest status` 출력, 그리고 memnest를 쓰는 호스트(pi, Claude Code, 다른 MCP 클라이언트, 일반 HTTP)를 적으면 문제의 절반은 설명됩니다.

보안 문제는 이슈 트래커에 올리지 마세요. [SECURITY.ko.md](SECURITY.ko.md)를 보세요.

## 라이선스

기여는 저장소에 적용되는 MIT 라이선스로 이뤄집니다. 풀 리퀘스트를 열면 그 조건으로 배포되는 데 동의하는 것입니다.
