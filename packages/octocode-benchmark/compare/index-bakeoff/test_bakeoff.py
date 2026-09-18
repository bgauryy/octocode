from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

import bakeoff


HERE = Path(__file__).resolve().parent


class ContractTests(unittest.TestCase):
    def test_cases_pin_both_required_repositories_and_cover_each_evidence_kind(self) -> None:
        contract = bakeoff.load_contract(HERE / "cases.json")
        self.assertEqual(
            contract["repositories"]["langchain"]["commit"],
            "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
        )
        self.assertEqual(
            contract["repositories"]["nextjs"]["commit"],
            "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
        )
        for repository in ("langchain", "nextjs"):
            kinds = {
                case["kind"]
                for case in contract["cases"]
                if case["repository"] == repository
            }
            self.assertEqual(kinds, {"content", "path", "symbol"})

    def test_normative_runs_require_at_least_thirty_paired_trials(self) -> None:
        with self.assertRaisesRegex(ValueError, "at least 30"):
            bakeoff.validate_trial_budget(29, normative=True)
        bakeoff.validate_trial_budget(30, normative=True)
        bakeoff.validate_trial_budget(1, normative=False)

    def test_correctness_gate_precedes_performance_and_rejects_unsupported_surface(self) -> None:
        records = [
            measurement("content", "linear-index", ["a:1:0"], 2.0),
            measurement("content", "live-scan", ["a:1:0"], 6.0),
            measurement("path", "linear-index", [], 1.0, status="unsupported"),
            measurement("path", "live-scan", ["a.ts"], 5.0),
        ]
        report = bakeoff.summarize(records, normative=False, requested_trials=1)
        self.assertEqual(report["correctness"]["parityCases"], ["content"])
        self.assertEqual(report["correctness"]["unsupportedCases"], ["path"])
        self.assertEqual(report["verdict"], "DISCARD_AS_S9_WINNER")
        self.assertIn("content", report["performance"])
        self.assertNotIn("path", report["performance"])

    def test_paired_ratio_is_computed_only_from_matching_trial_pairs(self) -> None:
        records = [
            measurement("content", "linear-index", ["a"], 2.0, trial=1),
            measurement("content", "live-scan", ["a"], 8.0, trial=1),
            measurement("content", "linear-index", ["a"], 4.0, trial=2),
            measurement("content", "live-scan", ["a"], 8.0, trial=2),
        ]
        report = bakeoff.summarize(records, normative=False, requested_trials=2)
        perf = report["performance"]["content"]
        self.assertEqual(perf["pairedTrials"], 2)
        self.assertEqual(perf["medianIndexToLiveWallRatio"], 0.375)

    def test_report_writer_marks_diagnostic_platform_as_non_normative(self) -> None:
        records = [
            measurement("content", "linear-index", ["a"], 2.0),
            measurement("content", "live-scan", ["a"], 3.0),
        ]
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "report.json"
            bakeoff.write_report(destination, records, normative=False, requested_trials=1)
            report = json.loads(destination.read_text())
        self.assertFalse(report["normative"])
        self.assertEqual(report["verdict"], "CONTINUE_DIAGNOSTIC_ONLY")


def measurement(
    case_id: str,
    arm: str,
    items: list[str],
    wall_ms: float,
    *,
    trial: int = 1,
    status: str = "ok",
) -> dict[str, object]:
    return {
        "caseId": case_id,
        "arm": arm,
        "trial": trial,
        "result": {"status": status, "items": items},
        "measurement": {
            "wall_time_ms": wall_ms,
            "memory": {"peak_process_tree_rss_bytes": 100},
        },
    }


if __name__ == "__main__":
    unittest.main()
