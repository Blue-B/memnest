# 실제 클라이언트 검증

<!-- markdownlint-disable MD013 -->

[English](client-validation.md) | [한국어](client-validation.ko.md)

재현 데모는 모델 계정 없이 돌릴 수 있도록 별도의 MCP 클라이언트와 HTTP 클라이언트를 씁니다. 2026-09-09에 한 두 번째 검증에서는 설치된 pi, Claude Code, Codex 프로그램을 격리된 Memnest 서비스와 작업공간에 대고 실행했습니다.

## 결과

| 클라이언트 | 결과 | 증거 범위 |
| --- | --- | --- |
| pi 0.85.1 | 통과 | 실제 pi 프로세스가 `memory_search`를 호출했고, 미리 넣은 고유 토큰과 `PI_RECALLED`를 출력했습니다. |
| Claude Code 2.1.258 | 도구 사용 전에 막힘 | 조직이 Claude Code의 Claude 구독 사용을 막아서 계정이 HTTP 403을 돌려줬습니다. |
| Codex CLI 0.139.0 | 도구 사용 전에 막힘 | 인증이 만료됐고, 그다음 계정 사용 한도에 도달했다고 나왔습니다. |

기계가 읽을 수 있는 요약은 [`demo-evidence/real-client-validation.json`](demo-evidence/real-client-validation.json)입니다. pi의 출력과 정리한 차단 메시지가 들어 있습니다. 계정 식별자, 요청 ID, 토큰, 로컬 경로는 뺐습니다.

이 결과는 실제 pi의 회상 경로 하나를 보여 줍니다. Claude Code에서 Codex로, 또는 Codex에서 Claude Code로 넘기는 것이나, 세 클라이언트 모두가 스스로 기억을 쓰는 것을 증명하지는 않습니다. 그 검증에는 동작하는 클라이언트 계정이 필요하고, 같은 절차를 따라야 합니다. 고유 표시를 넣거나 저장하고, 맞는 작업공간에서 새 클라이언트 프로세스를 시작하고, `memory_search` 도구 호출을 요구하고, 모델의 답만 믿지 말고 돌아온 텍스트를 비교합니다.

2026-11-03에는 Claude Code 2.1.312와 Codex 0.161.0으로 작업 카드를 이틀 동안 이어 가는 대화를 시험했습니다. 결과는 [작업 카드](task-cards.ko.md#claude-code와-codex-실제-도구)에 있습니다.
