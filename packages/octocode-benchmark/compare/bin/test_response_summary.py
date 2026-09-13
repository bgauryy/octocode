"""Exercise the recorder against native envelopes and external tool responses."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from instrument_command import _classify_outcome, _infer_result_count
from response_summary import summarize_response


def encode(value):
    return json.dumps(value).encode()


def native(data, **fields):
    return {"results": [{"index": 0, "data": data, **fields}]}


class ResponseSummaryTests(unittest.TestCase):
    def test_local_search_views_keep_comparable_count_units(self):
        stats = {"totalOccurrences": 3, "matchedLines": 2, "filesMatched": 1}
        cases = [
            ([{"path": "a", "matches": [{}, {}]}], {"totalFiles": 1, "totalMatches": 2}, 2, "match_rows"),
            ([{"path": "a"}], {"totalFiles": 1}, 1, "files"),
            ([{"path": "a", "totalOccurrences": 3}], {"totalFiles": 1, "totalMatches": 3}, 3, "occurrences"),
            ([{"path": "a", "totalMatchedLines": 2}], {"totalFiles": 1, "totalMatches": 2}, 2, "matched_lines"),
        ]
        for files, pagination, count, unit in cases:
            raw = encode(native({"searchEngine": "rg", "stats": stats, "files": files,
                                 "pagination": {"hasMore": False, **pagination}}))
            with self.subTest(unit=unit):
                summary = summarize_response(raw)
                self.assertEqual(summary["returned_result_count"], count)
                self.assertEqual(summary["total_result_count"], count)
                self.assertEqual(summary["result_unit"], unit)
                self.assertEqual(summary["queries"][0]["result_unit"], unit)

    def test_files_partial_total_never_borrows_occurrence_or_line_totals(self):
        raw = encode(native({"files": [{"path": "a"}],
                             "stats": {"totalOccurrences": 8, "matchedLines": 4},
                             "pagination": {"totalMatches": 8, "hasMore": True}}))
        summary = summarize_response(raw)
        self.assertEqual(summary["returned_result_count"], 1)
        self.assertIsNone(summary["total_result_count"])
        self.assertEqual(summary["result_unit"], "files")

    def test_heterogeneous_query_units_preserve_queries_without_summing(self):
        raw = encode({"results": [
            {"index": 0, "data": {"files": [{"path": "a", "matches": [{}, {}]}]}},
            {"index": 1, "data": {"files": [{"path": "a"}]}},
        ]})
        summary = summarize_response(raw)
        self.assertIsNone(summary["returned_result_count"])
        self.assertIsNone(summary["total_result_count"])
        self.assertIsNone(summary["result_unit"])
        self.assertTrue(summary["mixed_result_units"])
        self.assertEqual([query["returned_result_count"] for query in summary["queries"]], [2, 1])
        self.assertEqual([query["result_unit"] for query in summary["queries"]], ["match_rows", "files"])
        self.assertEqual(_classify_outcome(0, _infer_result_count(raw), None, raw=raw), "runtime-failed")

    def test_native_looking_rows_without_index_preserve_counts_but_fail_closed(self):
        cases = [
            ({"data": {"files": [{"path": "a", "matches": [{}, {}, {}]}]}}, 3, "runtime-failed"),
            ({"status": "empty", "data": {"stats": {"matchedLines": 0}}}, 0, "runtime-failed"),
            ({"status": "error", "data": {"error": "provider failed"}}, None, "runtime-failed"),
            ({"status": "error", "data": {"error": "unknown field"}}, None, "schema-invalid"),
        ]
        for row, count, outcome in cases:
            raw = encode({"results": [row]})
            with self.subTest(row=row):
                self.assertEqual(_infer_result_count(raw), count)
                self.assertEqual(_classify_outcome(0, count, "expected-absence", raw=raw), outcome)
                summary = summarize_response(raw)
                self.assertEqual(summary["query_count"], 1)
                self.assertFalse(summary["complete"])
                query = summary["queries"][0]
                self.assertIsNone(query["index"])
                self.assertEqual(query["response_position"], 0)
                self.assertTrue(query["malformed"])

    def test_per_file_pagination_preserves_hits_without_claiming_complete_results(self):
        for file_metadata in ({"pagination": {"hasMore": True}},
                              {"meta": {"diagnostics": {"partial": True}}}):
            raw = encode(native({"files": [{"path": "a", "matches": [{}], **file_metadata}]}))
            with self.subTest(file_metadata=file_metadata):
                self.assertEqual(_infer_result_count(raw), 1)
                self.assertEqual(_classify_outcome(0, 1, None, raw=raw), "runtime-failed")
                summary = summarize_response(raw)
                self.assertTrue(summary["partial"])
                self.assertFalse(summary["complete"])
                self.assertIsNone(summary["total_result_count"])

    def test_raw_arrays_and_match_source_metadata_remain_data(self):
        raw = encode([{"data": {"partial": True}}, {"index": 0, "status": "error"}])
        self.assertEqual(_infer_result_count(raw), 2)
        self.assertEqual(_classify_outcome(0, 2, None, raw=raw), "first-valid")
        raw = encode(native({"files": [{"path": "a", "matches": [
            {"pagination": {"hasMore": True}, "meta": {"diagnostics": {"partial": True}}}
        ]}]}))
        self.assertEqual(_classify_outcome(0, 1, None, raw=raw), "first-valid")

    def test_native_counts_matches_not_queries_or_global_totals(self):
        payload = native({
            "files": [{"path": "a", "matches": [{}, {}]}, {"path": "b", "matches": [{}]}],
            "stats": {"totalStructuralMatches": 9},
            "pagination": {"totalMatches": 9, "hasMore": True},
        })
        payload["shared"] = {"returnedMatchRows": 1, "totalMatchRows": 1}
        self.assertEqual(_infer_result_count(encode(payload)), 3)
        summary = summarize_response(encode(payload))
        self.assertEqual(summary["query_count"], 1)
        self.assertEqual(summary["total_result_count"], 9)
        self.assertTrue(summary["partial"])
        self.assertFalse(summary["complete"])

    def test_error_like_source_text_does_not_classify_success_as_failure(self):
        raw = encode(native({"matches": [{"error": "schema invalid", "partial": True}]}))
        self.assertEqual(_classify_outcome(0, _infer_result_count(raw), None, raw=raw), "first-valid")

    def test_complete_empty_native_response_honors_expected_absence(self):
        raw = encode(native({"stats": {"matchedLines": 0}}, status="empty"))
        self.assertEqual(_classify_outcome(0, _infer_result_count(raw), "expected-absence", raw=raw), "expected-empty")
        self.assertEqual(_classify_outcome(0, _infer_result_count(raw), None, raw=raw), "unproductive-empty")

    def test_unknown_query_does_not_disappear_from_mixed_batch(self):
        raw = encode({"results": [{"index": 0, "data": {"matches": [{}]}},
                                  {"index": 1, "data": {"futureShape": []}}]})
        self.assertIsNone(_infer_result_count(raw))
        self.assertEqual(_classify_outcome(0, None, None, raw=raw), "runtime-failed")
        summary = summarize_response(raw)
        self.assertEqual(summary["query_count"], 2)
        self.assertEqual(summary["queries"][0]["returned_result_count"], 1)
        self.assertIsNone(summary["queries"][1]["returned_result_count"])

    def test_native_empty_is_zero_but_an_error_without_evidence_is_unknown(self):
        self.assertEqual(_infer_result_count(
            encode(native({"stats": {"matchedLines": 0}}, status="empty"))), 0)
        self.assertIsNone(_infer_result_count(
            encode(native({"error": "provider failed"}, status="error"))))

    def test_error_in_a_batch_does_not_turn_known_matches_into_a_complete_count(self):
        raw = encode({"results": [
            {"index": 0, "data": {"matches": [{}, {}]}},
            {"index": 1, "status": "error", "data": {"error": "provider failed"}},
        ]})
        summary = summarize_response(raw)
        self.assertEqual(summary["queries"][0]["returned_result_count"], 2)
        self.assertIsNone(summary["queries"][1]["returned_result_count"])
        self.assertIsNone(summary["returned_result_count"])

    def test_success_exit_does_not_hide_query_failure(self):
        cases = [
            (native({"error": "provider failed"}, status="error"), "runtime-failed"),
            (native({"error": "unknown field: directory"}, status="error"), "schema-invalid"),
            ({"response": {"error": {"code": -1, "message": "server failed"}}}, "runtime-failed"),
        ]
        for payload, expected in cases:
            raw = encode(payload)
            with self.subTest(payload=payload):
                self.assertEqual(_classify_outcome(0, _infer_result_count(raw), None, raw=raw), expected)

    def test_unknown_shapes_fail_closed(self):
        for raw in (b"plain text", b'{"mystery":42}', b'{"count":true}',
                    b'{"type":"mystery"}\n{"type":"other"}',
                    encode(native({"mystery": [1, 2]}))):
            with self.subTest(raw=raw):
                self.assertIsNone(_infer_result_count(raw))
                self.assertEqual(_classify_outcome(0, None, None, raw=raw), "runtime-failed")

    def test_partial_results_are_not_full_success_or_expected_absence(self):
        for fields in ({"meta": {"diagnostics": {"partial": True}}}, {"status": "partial"}):
            raw = encode(native({"matches": [{}]}, **fields))
            self.assertEqual(_classify_outcome(0, 1, None, raw=raw), "runtime-failed")
        raw = encode(native({"matches": [], "pagination": {"hasMore": True}}, status="empty"))
        self.assertEqual(_classify_outcome(0, 0, "expected-absence", raw=raw), "runtime-failed")

    def test_known_external_responses_remain_countable(self):
        cases = [
            ([{"text": "a"}, {"text": "b"}], 2),
            ({"results": [{"path": "a"}]}, 1),
            ({"data": {"matches": [{}]}}, 1),
            ({"response": {"result": [{"uri": "file:///a"}]}}, 1),
            ({"response": {"result": None}}, 0),
            ({"response": {"result": {"uri": "file:///a", "range": {}}}}, 1),
            ({"type": "summary", "data": {"stats": {"matches": 0}}}, 0),
        ]
        for payload, expected in cases:
            with self.subTest(payload=payload):
                self.assertEqual(_infer_result_count(encode(payload)), expected)
        raw = b'{"type":"begin"}\n{"type":"match"}\n{"type":"end"}\n{"type":"summary"}\n'
        self.assertEqual(_infer_result_count(raw), 1)

    def test_mixed_batch_records_per_query_evidence(self):
        payload = {"results": [
            {"index": 0, "data": {"matches": [{}, {}], "pagination": {"totalMatches": 2, "hasMore": False}}},
            {"index": 1, "status": "empty", "data": {"stats": {"matchedLines": 0}}},
            {"index": 2, "status": "error", "data": {"error": "provider failed"}},
        ]}
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "record.jsonl"
            result = subprocess.run([
                sys.executable, str(Path(__file__).with_name("instrument_command.py")),
                "--log", str(log), "--artifact-dir", str(Path(tmp) / "artifacts"),
                "--label", "fixture:mixed", "--", sys.executable, "-c", f"print({json.dumps(payload)!r})",
            ], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            record = json.loads(log.read_text())
            self.assertIsNone(record["result_count"])
            self.assertEqual(record["call_outcome"], "runtime-failed")
            summary = record["response_summary"]
            self.assertEqual(summary["query_count"], 3)
            self.assertIsNone(summary["returned_result_count"])
            self.assertIsNone(summary["total_result_count"])
            self.assertEqual([q["status"] for q in summary["queries"]], ["success", "empty", "error"])
            self.assertFalse(summary["complete"])


if __name__ == "__main__":
    unittest.main()
