"""Measurement regressions: missing data, invalid attempts and distinct corpora."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import campaign_summary as campaign
import matrix_summary as matrix


class SummaryTest(unittest.TestCase):
    def test_source_json_or_non_whole_scores_are_not_judgments(self):
        valid = {"X": {"correctness": 10, "depth": 5, "workflow": 5},
                 "Y": {"correctness": 9, "depth": 4, "workflow": 4}, "winner": "tie", "reason": "same evidence"}
        text = lambda value: '```json\n' + json.dumps(value) + '\n```'
        self.assertEqual(campaign.parse_verdict(text(valid)), valid)
        unknown_workflow = {**valid, "X": {**valid["X"], "workflow": None}}
        self.assertIsNone(campaign.parse_verdict(text(unknown_workflow))["X"]["workflow"])
        for invalid in ({"dependencies": {}}, {**valid, "X": {**valid["X"], "correctness": 9.5}},
                        {**valid, "Y": {**valid["Y"], "correctness": 11}}):
            with self.assertRaises(ValueError):
                campaign.parse_verdict(text(invalid))

    def test_successful_envelope_does_not_hide_query_error_or_ranking_skip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "transcript.jsonl"
            event = {"type": "item.completed", "item": {"type": "mcp_tool_call", "tool": "lspSearch",
                "status": "completed", "result": {"structured_content": {"results": [
                    {"status": "error", "data": {"errorCode": "anchorUnresolved"}},
                    {"data": {"ranking": {"status": "skipped", "reason": "identityOrder", "totalItems": 1}}}]}}}}
            path.write_text(json.dumps(event))
            result = campaign.codex_receipts(path)
            self.assertEqual(result["query_errors"], [{"tool": "lspSearch", "code": "anchorUnresolved", "count": 1}])
            self.assertEqual(result["ranking_receipts"][0]["status"], "skipped")

    def test_provider_failure_and_missing_usage_stay_in_denominator(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "http.jsonl"
            events = [{"event": "started", "id": i} for i in (1, 2, 3)] + [
                {"event": "finished", "id": 1, "httpStatus": 200, "inputTokens": 10, "outputTokens": 2, "resolvedModel": "fixed"},
                {"event": "finished", "id": 2, "httpStatus": 429, "inputTokens": None, "outputTokens": None}]
            path.write_text("\n".join(json.dumps(event) for event in events))
            summary = campaign.classification_http(path)
            self.assertEqual(summary["attempts"], 3)
            self.assertEqual(summary["completed"], 2)
            self.assertEqual(summary["failed_or_unknown"], 2)
            self.assertEqual(summary["known_input_tokens"], 10)
            self.assertFalse(summary["input_tokens_complete"])

    def test_status_gate_and_unknown_usage_are_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            run = root / "runs/mcp-p1-Q1"
            run.mkdir(parents=True)
            (run / "result.json").write_text(json.dumps({"subtype": "success", "duration_ms": 500}))
            (run / "status.json").write_text(json.dumps({"ok": False, "problems": ["private path was readable"]}))
            log = root / "mcp-p1-Q1.jsonl"
            log.write_text(json.dumps({"kind": "answer", "model_in_chars": 0, "model_out_chars": 10, "total_chars": 10}))
            result = campaign.run_row(root, log)
            self.assertFalse(result["ok"])
            self.assertEqual(result["secs"], 0.5)
            self.assertIsNone(result["model_secs"])
            self.assertIsNone(result["tool_secs"])
            self.assertIsNone(result["input_tokens"])
            self.assertIsNone(result["output_tokens"])
            self.assertIsNone(result["uncached_input_tokens"])
            self.assertEqual(result["problems"], ["private path was readable"])

    def test_classifiers_and_replays_are_distinct_from_clasify_tool_calls(self):
        calls = [
            {"cmd": 'localFetch {"queries":[{"classifiers":["relevant"]},{"classifiers":["relevant"],"ranking":"plan"}]}'},
            {"cmd": 'clasify {"queries":[{}]}'},
            {"cmd": 'localFetch {"queries":[{"path":"exact"}]}'},
        ]
        result = campaign.semantic_calls(calls)
        self.assertEqual(result["goal_queries"], 2)
        self.assertEqual(result["ranking_replay_queries"], 1)
        self.assertEqual(result["initial_goal_queries"], 1)
        self.assertEqual(result["tool_calls"], {"localFetch": 2, "clasify": 1})

    def test_historical_goals_transcripts_still_count_as_classifier_queries(self):
        calls = [{"cmd": 'localSearch {"queries":[{"goals":["relevant"]}]}'}]
        self.assertEqual(campaign.semantic_calls(calls)["goal_queries"], 1)

    def test_current_classification_counts_without_guessing_opaque_cursor_state(self):
        calls = [
            {"cmd": 'localFetch {"queries":[{"path":"source.rs","classification":{"predicates":["owns validation"]}}]}'},
            {"cmd": 'localFetch {"cursor":"opaque"}'},
        ]
        result = campaign.semantic_calls(calls)
        self.assertEqual(result["goal_queries"], 1)
        self.assertEqual(result["initial_goal_queries"], 1)
        self.assertEqual(result["ranking_replay_queries"], 0)
        self.assertEqual(result["tool_calls"], {"localFetch": 2})

    def test_current_selection_receipt_preserves_partition_and_unknown_usage(self):
        selection = {"status": "partial", "scope": "selectedView", "unit": "chunks",
                     "totalItems": 3, "selectedItems": 1, "excludedItems": 1, "unknownItems": 1,
                     "failures": [{"candidate": 2, "errorCode": "providerUnavailable", "error": "offline"}],
                     "usage": {"calls": 2, "knownUsageCalls": 1, "unknownUsageCalls": 1,
                               "inputTokens": 10, "outputTokens": 2, "replayedEvaluations": 0}}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "transcript.jsonl"
            path.write_text(json.dumps({"type": "item.completed", "item": {
                "type": "mcp_tool_call", "tool": "localFetch", "result": {
                    "structured_content": {"results": [{"data": {"selection": selection}}]}}}}))
            result = campaign.codex_receipts(path)
        self.assertEqual(result["selection_receipts"], [{"tool": "localFetch", **selection}])
        self.assertEqual(result["ranking_receipts"], [])

    def test_two_mcp_arms_never_collapse_and_other_corpus_is_excluded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "overview").mkdir()
            (root / "overview/per_question_summary.json").write_text('{"rows":[]}')
            for suite in ("github-v2", "local-goals-v2"):
                rows = [{"arm": arm, "pass": 1, "q": "Q1", "chars": chars, "ok": True}
                        for arm, chars in (("mcp", 100), ("mcp-clasify", 50))]
                doc = {"campaign": suite, "question_set": suite, "octocode_label": "local build",
                       "runner_model": "terra", "runs": rows, "judgments": []}
                (root / suite).mkdir()
                (root / suite / "run.json").write_text(json.dumps(doc))
            with patch.object(matrix, "RESULTS", root):
                groups, chars, _, _, _ = matrix.load_groups()
            self.assertEqual(len(groups), 2)
            self.assertEqual(sorted(sum(chars[g]["Q1"]) for g in groups), [50, 100])
            self.assertTrue(all(g["campaigns"] == ["github-v2"] for g in groups.values()))

    def test_invalid_run_is_not_a_smaller_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "overview").mkdir()
            (root / "overview/per_question_summary.json").write_text('{"rows":[]}')
            doc = {"campaign": "invalid", "question_set": "github-v2", "octocode_label": "build",
                   "runner_model": "terra", "judgments": [], "runs": [
                       {"arm": "mcp", "pass": 1, "q": "Q1", "chars": 1, "ok": False}]}
            (root / "invalid").mkdir()
            (root / "invalid/run.json").write_text(json.dumps(doc))
            with patch.object(matrix, "RESULTS", root):
                groups, chars, _, _, _ = matrix.load_groups()
            group = next(iter(groups))
            self.assertEqual(chars[group]["Q1"], [])
            self.assertEqual(len(groups[group]["runs"]), 1)

    def test_head_to_head_uses_declared_one_point_tie(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "overview").mkdir()
            rows, judgments = [], []
            for n, (a, b) in enumerate(((10, 9), (8, 10), (10, 8), (9, 9)), 1):
                q = f"Q{n}"
                rows.extend({"arm": arm, "pass": 1, "q": q, "chars": 100, "ok": True}
                            for arm in ("mcp", "mcp-clasify"))
                judgments.append({"pair": "mcp~mcp-clasify", "pass": 1, "q": q,
                    "scores": {"mcp": {"correctness": a}, "mcp-clasify": {"correctness": b}},
                    "winner": "tie"})
            doc = {"campaign": "paired", "question_set": "local", "octocode_label": "build",
                   "runner_model": "terra", "runs": rows, "judgments": judgments}
            (root / "paired").mkdir()
            (root / "paired/run.json").write_text(json.dumps(doc))
            with patch.object(matrix, "RESULTS", root):
                _, _, _, pairs, _ = matrix.load_groups("local")
            self.assertEqual((pairs[0]["a_more_correct"], pairs[0]["same"], pairs[0]["b_more_correct"]), (1, 2, 1))

    def test_newer_dated_campaign_is_selected_after_older_timed_campaign(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "overview").mkdir()
            for name in ("new-2026-10-13", "old-235959-2026-10-12"):
                doc = {"campaign": name, "question_set": "local", "octocode_label": name,
                       "runner_model": "terra", "judgments": [], "runs": [
                           {"arm": "mcp", "pass": 1, "q": "Q1", "chars": 100, "ok": True}]}
                (root / name).mkdir()
                (root / name / "run.json").write_text(json.dumps(doc))
            with patch.object(matrix, "RESULTS", root):
                groups, *_ = matrix.load_groups("local")
            self.assertEqual(list(groups.values())[-1]["campaigns"], ["new-2026-10-13"])


if __name__ == "__main__":
    unittest.main()
