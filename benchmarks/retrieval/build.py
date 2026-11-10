"""Turn dataset files into ingestable groups.

One group = one memnest project = one conversation (LoCoMo) or one question's
haystack (LongMemEval). A group has `turns` (one memory per dialog turn) and the
`questions` that may only search that project.
"""

import random
import re

# ---------------------------------------------------------------- formatting


def render(label, text, date, date_in_text):
    """Document text of one turn.

    The transcript watcher (core/src/watch.rs) stores "User said: <text>" or
    "Assistant answered: <text>" and no date. The date variant prepends the
    session date, because the watcher's created_at is the ingest time and would
    carry no information in a benchmark replay.
    """
    body = f"{label}: {text}"
    return f"[{date}] {body}" if date_in_text else body


# -------------------------------------------------------------------- LoCoMo

EVIDENCE_RE = re.compile(r"D:?(\d+):(\d+)")
LOCOMO_CATEGORIES = {
    1: "multi-hop",
    2: "temporal",
    3: "open-domain",
    4: "single-hop",
    5: "adversarial",
}


def normalize_evidence(raw):
    """Fix the malformed evidence strings found in locomo10.json
    ('D9:1 D4:4', 'D8:6; D9:17', 'D30:05', 'D:11:26') into 'D<n>:<m>' ids."""
    found = []
    for item in raw:
        for session, turn in EVIDENCE_RE.findall(item):
            found.append(f"D{int(session)}:{int(turn)}")
    return found


def locomo_groups(data, date_in_text):
    groups = []
    for index, sample in enumerate(data):
        conversation = sample["conversation"]
        turns = []
        known = set()
        session_numbers = sorted(
            int(key.split("_")[1])
            for key in conversation
            if re.fullmatch(r"session_\d+", key)
        )
        for number in session_numbers:
            date = conversation.get(f"session_{number}_date_time", "")
            for turn in conversation[f"session_{number}"]:
                known.add(turn["dia_id"])
                turns.append(
                    {
                        "key": turn["dia_id"],
                        "session": f"session_{number}",
                        "event_id": f"{turn['dia_id']}",
                        "role": None,
                        "text": render(
                            f"{turn['speaker']} said", turn["text"], date, date_in_text
                        ),
                    }
                )
        questions = []
        dropped = 0
        for q_index, qa in enumerate(sample["qa"]):
            evidence = normalize_evidence(qa.get("evidence", []))
            valid = [e for e in dict.fromkeys(evidence) if e in known]
            dropped += len(set(evidence)) - len(valid)
            questions.append(
                {
                    "id": f"{sample['sample_id']}#{q_index}",
                    "question": qa["question"],
                    "category": LOCOMO_CATEGORIES[qa["category"]],
                    "category_number": qa["category"],
                    "relevant": valid,
                    "evidence_raw": qa.get("evidence", []),
                }
            )
        groups.append(
            {
                "project": f"locomo-{index}-{sample['sample_id']}",
                "turns": turns,
                "questions": questions,
                "unknown_evidence_dropped": dropped,
            }
        )
    return groups


# -------------------------------------------------------------- LongMemEval


def lme_groups(data, date_in_text, indices):
    groups = []
    for index in indices:
        item = data[index]
        turns = []
        seen_sessions = set()
        repeated_sessions = 0
        for session_id, date, session in zip(
            item["haystack_session_ids"],
            item["haystack_dates"],
            item["haystack_sessions"],
            strict=True,
        ):
            # 13 questions repeat a byte-identical session id in the haystack;
            # a transcript replay of the same event is deduplicated by memnest.
            if session_id in seen_sessions:
                repeated_sessions += 1
                continue
            seen_sessions.add(session_id)
            for position, turn in enumerate(session):
                label = "User said" if turn["role"] == "user" else "Assistant answered"
                turns.append(
                    {
                        "key": f"{session_id}#{position}",
                        "session": session_id,
                        "event_id": f"{session_id}#{position}",
                        "role": turn["role"],
                        "has_answer": bool(turn.get("has_answer")),
                        "text": render(label, turn["content"], date, date_in_text),
                    }
                )
        abstention = item["question_id"].endswith("_abs")
        groups.append(
            {
                "project": f"lme-{item['question_id']}",
                "turns": turns,
                "repeated_sessions_skipped": repeated_sessions,
                "questions": [
                    {
                        "id": item["question_id"],
                        "question": item["question"],
                        "category": item["question_type"],
                        "abstention": abstention,
                        "relevant_sessions": item["answer_session_ids"],
                        "relevant": [t["key"] for t in turns if t["has_answer"]],
                    }
                ],
            }
        )
    return groups


def stratified_sample(data, size, seed):
    """Fixed-seed sample of non-abstention questions, proportional to each
    question_type (largest remainder), at least one per type. Returns sorted
    indices into `data`."""
    by_type = {}
    for index, item in enumerate(data):
        if not item["question_id"].endswith("_abs"):
            by_type.setdefault(item["question_type"], []).append(index)
    total = sum(len(v) for v in by_type.values())
    types = sorted(by_type)
    quota = {t: max(1, int(size * len(by_type[t]) / total)) for t in types}
    remainders = sorted(
        types, key=lambda t: size * len(by_type[t]) / total - int(size * len(by_type[t]) / total), reverse=True
    )
    position = 0
    while sum(quota.values()) < size:
        quota[remainders[position % len(remainders)]] += 1
        position += 1
    rng = random.Random(seed)
    chosen = []
    for question_type in types:
        chosen += rng.sample(by_type[question_type], min(quota[question_type], len(by_type[question_type])))
    return sorted(chosen)
