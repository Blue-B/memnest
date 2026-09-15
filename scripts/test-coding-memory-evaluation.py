#!/usr/bin/env python3
"""Metric and fixed-judgment integrity regressions; independent of retrieval outputs."""

import importlib.util
import json
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "evaluation", HERE / "evaluate-coding-memory.py"
)
assert SPEC and SPEC.loader
EVALUATION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EVALUATION)


class EvaluationTests(unittest.TestCase):
    def test_metrics_count_misses_and_false_positives(self):
        rows = [
            {"relevant": ["a"], "retrieved": ["a", "b"]},
            {"relevant": ["b"], "retrieved": ["x", "b"]},
            {"relevant": ["c"], "retrieved": []},
            {"relevant": [], "retrieved": ["x"]},
            {"relevant": [], "retrieved": []},
        ]
        self.assertEqual(
            EVALUATION.summarize(rows, "retrieved"),
            {
                "positive_cases": 3,
                "hit_at_1": 1 / 3,
                "recall_at_5": 2 / 3,
                "mrr_at_5": 0.5,
                "negative_cases": 2,
                "negative_empty_rate": 0.5,
            },
        )

    def test_keyword_file_and_unicode_tokens(self):
        self.assertEqual(
            EVALUATION.tokens("server/auth.rs 예약 UTC UTC"),
            {"server", "auth", "rs", "예약", "utc"},
        )

    def test_judgments_are_visible_and_project_local(self):
        fixture_path = HERE / "fixtures/coding-memory.json"
        try:
            fixture = json.loads(fixture_path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            self.fail(f"could not load fixed evaluation fixture: {error}")
        records = {r["key"]: r for r in fixture["records"]}
        self.assertEqual(len(records), len(fixture["records"]))
        self.assertEqual(len({c["id"] for c in fixture["cases"]}), 48)
        self.assertEqual(sum(bool(c["relevant"]) for c in fixture["cases"]), 22)
        self.assertEqual(sum(not c["relevant"] for c in fixture["cases"]), 26)
        obsolete = {r["supersedes"] for r in records.values() if "supersedes" in r}
        for case in fixture["cases"]:
            for key in case["relevant"]:
                self.assertNotIn(key, obsolete)
                self.assertEqual(records[key]["project"], case["project"])


if __name__ == "__main__":
    unittest.main()
