#!/usr/bin/env python3
"""Deterministic tests for Terra v3 comparison aggregation."""

from __future__ import annotations

import unittest
from pathlib import Path
import sys
import json
import subprocess
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from terra_v3_report import build_comparison


def _run(arm: str, pass_number: int, wall_ms: int, tokens: int, passed: bool) -> dict[str, object]:
    return {
        "caseId": "p01",
        "arm": arm,
        "pass": pass_number,
        "cacheCohort": "cold",
        "anchorResult": {
            "passed": passed,
            "kind": "exact-result-set",
            "expectedDigest": "a" * 64,
            "actualDigest": "a" * 64 if passed else "b" * 64,
        },
        "metrics": {
            "wallMs": wall_ms,
            "cpuUserMs": 10,
            "cpuSystemMs": 2,
            "peakRssBytes": 1_000,
            "ioReadBytes": 100,
            "ioWriteBytes": 20,
            "contextCharacters": 500,
        },
        "tokenUsage": {
            "inputTokens": tokens - 20,
            "outputTokens": 20,
            "totalTokens": tokens,
            "cachedInputTokens": 0,
            "reasoningTokens": 5,
            "source": "provider",
        },
        "trajectory": {"summary": {
            "totalCalls": 2,
            "firstValidCalls": 1,
            "schemaInvalidCalls": 0,
            "runtimeFailedCalls": 0,
            "productiveSuccessCalls": 0,
            "unproductiveEmptyCalls": 0,
            "expectedEmptyCalls": 1,
            "repairedSchemaCalls": 0,
        }},
    }


class TerraV3ReportTests(unittest.TestCase):
    def test_keeps_quality_performance_tokens_and_trajectory_separate(self) -> None:
        runs = [
            _run("octocode", 1, 100, 80, True),
            _run("octocode", 2, 120, 100, True),
            _run("ripgrep", 1, 50, 120, True),
            _run("ripgrep", 2, 70, 140, False),
        ]
        judges = [
            {
                "judgeId": "judge-p01-1",
                "caseId": "p01",
                "pass": 1,
                "scores": {
                    "X": {"correctness": 9, "depth": 4, "workflow": 5},
                    "Y": {"correctness": 8, "depth": 3, "workflow": 4},
                },
            },
            {
                "judgeId": "judge-p01-2",
                "caseId": "p01",
                "pass": 2,
                "scores": {
                    "X": {"correctness": 6, "depth": 3, "workflow": 3},
                    "Y": {"correctness": 10, "depth": 5, "workflow": 5},
                },
            },
        ]
        assignments = [
            {"judgeId": "judge-p01-1", "caseId": "p01", "pass": 1, "labelToArm": {"X": "octocode", "Y": "ripgrep"}},
            {"judgeId": "judge-p01-2", "caseId": "p01", "pass": 2, "labelToArm": {"X": "ripgrep", "Y": "octocode"}},
        ]

        role_receipts = [
            {
                "role": "judge",
                "tokenUsage": {
                    "source": "provider", "inputTokens": 100, "outputTokens": 20,
                    "totalTokens": 120, "cachedInputTokens": 0, "reasoningTokens": 5,
                },
            },
            {
                "role": "confirmationJudge",
                "tokenUsage": {
                    "source": "provider", "inputTokens": 110, "outputTokens": 20,
                    "totalTokens": 130, "cachedInputTokens": 0, "reasoningTokens": 5,
                },
            },
        ]
        report = build_comparison({
            "runs": runs, "judges": judges, "blindAssignments": assignments,
            "roleReceipts": role_receipts,
        })
        octocode = report["arms"]["octocode"]
        ripgrep = report["arms"]["ripgrep"]

        self.assertEqual(octocode["quality"]["deterministicPassRate"], 1.0)
        self.assertEqual(ripgrep["quality"]["deterministicPassRate"], 0.5)
        self.assertEqual(octocode["quality"]["judge"]["correctnessMean"], 9.5)
        self.assertEqual(ripgrep["quality"]["judge"]["correctnessMean"], 7.0)
        self.assertEqual(octocode["performance"]["wallMs"]["median"], 110.0)
        self.assertEqual(ripgrep["performance"]["wallMs"]["median"], 60.0)
        self.assertEqual(octocode["tokens"]["totalTokens"]["sum"], 180)
        self.assertEqual(ripgrep["tokens"]["totalTokens"]["sum"], 260)
        self.assertEqual(octocode["trajectory"]["totalCalls"], 4)
        self.assertEqual(report["meta"]["tokenMeasurement"], "provider-reported")
        self.assertEqual(report["campaignOverheadTokens"]["judge"]["totalTokens"]["sum"], 120)
        self.assertEqual(report["campaignOverheadTokens"]["confirmationJudge"]["totalTokens"]["sum"], 130)
        pairwise = report["pairwise"]["octocode-vs-ripgrep"]
        self.assertGreater(pairwise["tokenRatio"]["geometricMean"], 1.0)
        self.assertLess(pairwise["wallRatio"]["geometricMean"], 1.0)
        self.assertIsNotNone(pairwise["tokenRatio"]["ci95Low"])
        self.assertIn("peakRssBytes", pairwise["performanceRatios"])
        self.assertIn("inputTokens", pairwise["tokenRatios"])
        self.assertEqual(octocode["trajectory"]["runtimeFailedCalls"], 0)
        self.assertEqual(octocode["trajectory"]["expectedEmptyCalls"], 2)
        self.assertEqual(octocode["trajectory"]["unproductiveEmptyRate"], 0.0)

    def test_rejects_estimated_tokens_and_inconsistent_totals(self) -> None:
        estimated = _run("octocode", 1, 100, 80, True)
        estimated["tokenUsage"]["source"] = "estimated"
        with self.assertRaisesRegex(ValueError, "provider-reported"):
            build_comparison({"runs": [estimated], "judges": []})

        inconsistent = _run("octocode", 1, 100, 80, True)
        inconsistent["tokenUsage"]["totalTokens"] = 999
        with self.assertRaisesRegex(ValueError, "totalTokens"):
            build_comparison({"runs": [inconsistent], "judges": []})

    def test_reports_cache_cohorts_independently(self) -> None:
        cold = _run("octocode", 1, 200, 80, True)
        warm = _run("octocode", 2, 40, 80, True)
        warm["cacheCohort"] = "warm-index"
        report = build_comparison({"runs": [cold, warm], "judges": []})
        cohorts = report["arms"]["octocode"]["performance"]["cacheCohorts"]
        self.assertEqual(cohorts["cold"]["wallMs"]["median"], 200.0)
        self.assertEqual(cohorts["warm-index"]["wallMs"]["median"], 40.0)

    def test_cli_refuses_to_report_an_unvalidated_unsealed_campaign(self) -> None:
        here = Path(__file__).resolve().parent
        suite_root = here.parent / "terra-v3"
        with tempfile.TemporaryDirectory() as tmp:
            campaign = Path(tmp) / "campaign.json"
            campaign.write_text(json.dumps({"runs": [_run("octocode", 1, 10, 10, True)]}), encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable, str(here / "terra_v3_report.py"),
                    "--campaign", str(campaign),
                    "--contracts", str(suite_root / "contracts"),
                    "--suite", str(suite_root / "suite"),
                ],
                text=True, capture_output=True,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("campaign-ready", result.stdout)


if __name__ == "__main__":
    unittest.main()
