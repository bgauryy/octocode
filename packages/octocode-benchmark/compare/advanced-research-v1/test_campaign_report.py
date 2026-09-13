"""Regression coverage for reusable sealed-campaign verification."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import campaign_report


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def receipt(case, arm, pass_number, *, usage=True):
    native = {"input_tokens": 100, "cached_input_tokens": 40,
              "output_tokens": 20, "reasoning_output_tokens": 8}
    events = [{"type": "item.completed", "item": {"id": "one", "type": "command_execution",
               "command": "node tool", "aggregated_output": "abc", "exit_code": 0, "status": "completed"}}]
    if usage:
        events.append({"type": "turn.completed", "usage": native})
    result = {
        "case": case, "arm": arm, "passNumber": pass_number, "eligible": True, "failures": [],
        "exitCode": 0, "timedOut": False, "protocol": "historical-v1",
        "nativeUsageReceipts": [native] if usage else [],
        "usage": {"inputTokens": 100, "cachedInputTokens": 40, "outputTokens": 20,
                  "reasoningOutputTokens": 8, "cacheWriteInputTokens": None,
                  "totalTokens": 120, "uncachedInputTokens": 60},
        "commands": [{"id": "one", "rawOutputBytes": 3}], "shellCalls": 1,
        "shellCallCountKind": "observed_unique_command_execution_item_ids",
        "rawToolOutputBytes": 3, "wallSeconds": 2.0,
    }
    return result, events


def campaign(root, *, missing_usage=False, arms=("alpha", "beta")):
    results = []
    for arm in arms:
        row, events = receipt("A01", arm, 1, usage=not (missing_usage and arm == "beta"))
        trial = root / f"A01-p01-{arm}"
        trial.mkdir()
        (trial / "events.jsonl").write_text(
            "".join(json.dumps(event) + "\n" for event in events), encoding="utf-8"
        )
        (trial / "answer.md").write_text(arm, encoding="utf-8")
        row["artifacts"] = {name: digest(trial / name) for name in ("events.jsonl", "answer.md")}
        results.append(row)
    report = {"schemaVersion": 1, "plan": {"protocol": "historical-v1", "trials": 2,
              "cases": ["A01"], "passes": 1, "budgets": {}}, "candidateUnchanged": True,
              "fatalError": None, "results": results,
              "pairs": [{"case": "A01", "pass": 1, "eligible": True}], "winner": None}
    write_json(root / "report.json", report)
    write_json(root / "manifest.json", {"report.json": digest(root / "report.json")})
    return report


class CampaignReportTests(unittest.TestCase):
    def test_verified_campaign_reports_scheduled_completed_eligible_and_ungraded_ratios(self):
        with tempfile.TemporaryDirectory() as directory:
            report = campaign(Path(directory))
            actual = campaign_report.verify_campaign(Path(directory))
        self.assertEqual(actual["errors"], [], actual)
        self.assertEqual(actual["scheduledTrials"], 2)
        self.assertEqual(actual["completedTrials"], 2)
        self.assertEqual(actual["eligibleTrials"], 2)
        self.assertEqual(actual["usage"]["knownTrials"], 2)
        self.assertEqual(actual["arms"]["alpha"]["inputTokens"], 100)
        self.assertEqual(actual["pairs"][0]["quality"], "ungraded")
        self.assertEqual(actual["pairs"][0]["costRatios"]["alpha/beta"]["totalTokens"], 1.0)
        self.assertNotIn("winner", actual)
        self.assertFalse(actual["auditReplay"]["attempted"])
        self.assertEqual(report["winner"], None)

    def test_tampered_trial_artifact_and_command_receipt_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            campaign(root)
            (root / "A01-p01-alpha" / "answer.md").write_text("tampered", encoding="utf-8")
            report = json.loads((root / "report.json").read_text())
            report["results"][1]["rawToolOutputBytes"] = 4
            report["results"][1]["shellCalls"] = 2
            write_json(root / "report.json", report)
            write_json(root / "manifest.json", {"report.json": digest(root / "report.json")})
            actual = campaign_report.verify_campaign(root)
        self.assertTrue(any("artifact_hash" in error for error in actual["errors"]))
        self.assertTrue(any("rawToolOutputBytes" in error for error in actual["errors"]))
        self.assertTrue(any("shellCalls" in error for error in actual["errors"]))

    def test_missing_native_usage_and_partial_schedule_are_reported_separately(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = campaign(root, missing_usage=True)
            report["plan"]["trials"] = 4
            write_json(root / "report.json", report)
            write_json(root / "manifest.json", {"report.json": digest(root / "report.json")})
            actual = campaign_report.verify_campaign(root)
        self.assertEqual(actual["scheduledTrials"], 4)
        self.assertEqual(actual["completedTrials"], 2)
        self.assertEqual(actual["eligibleTrials"], 1)
        self.assertEqual(actual["usage"]["unknownTrials"], 1)
        self.assertTrue(any("native_usage_receipt_count" in error for error in actual["errors"]))

    def test_multiple_native_receipts_cannot_be_summed_into_a_trial_total(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            campaign(root)
            events_path = root / "A01-p01-alpha" / "events.jsonl"
            events = [json.loads(line) for line in events_path.read_text().splitlines()]
            events.append({"type": "turn.completed", "usage": {"input_tokens": 1, "output_tokens": 1}})
            events_path.write_text("".join(json.dumps(event) + "\n" for event in events), encoding="utf-8")
            report = json.loads((root / "report.json").read_text())
            report["results"][0]["artifacts"]["events.jsonl"] = digest(events_path)
            write_json(root / "report.json", report)
            write_json(root / "manifest.json", {"report.json": digest(root / "report.json")})
            actual = campaign_report.verify_campaign(root)
        self.assertTrue(any("native_usage_receipt_count=2" in error for error in actual["errors"]))
        self.assertEqual(actual["usage"]["unknownTrials"], 1)

    def test_main_writes_optional_report_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            campaign(root)
            output = root / "verification.json"
            self.assertEqual(campaign_report.main(["--campaign", str(root), "--output", str(output)]), 0)
            self.assertTrue(json.loads(output.read_text())["verified"])

    def test_current_protocol_replays_a_real_field_form_with_its_frozen_bridge(self):
        import cli_input
        import pilot

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            corpus = root / "corpus"
            corpus.mkdir()
            source = corpus / "fixture.py"
            source.write_text("value = 1\n", encoding="utf-8")
            report = campaign(root, arms=("octocode", "raw-tools"))
            bridge = cli_input.prepare_bridge(root)
            cli = pilot.WORKSPACE / "packages/octocode/out/octocode.js"
            for row in report["results"]:
                row["protocol"] = pilot.PROTOCOL
                row["budgets"] = {name: getattr(pilot.Budgets(), name) for name in pilot.Budgets.__dataclass_fields__}
                events_path = root / f"{row['case']}-p{row['passNumber']:02}-{row['arm']}" / "events.jsonl"
                events = [json.loads(line) for line in events_path.read_text().splitlines()]
                events[0]["item"]["command"] = (
                    f"node {cli} tools localFetch --path {source}"
                    if row["arm"] == "octocode" else f"rg value {source}"
                )
                events.append({"type": "item.completed", "item": {
                    "id": "answer", "type": "agent_message", "text": "Answer fixture.py:1"
                }})
                events_path.write_text("".join(json.dumps(event) + "\n" for event in events), encoding="utf-8")
                row["artifacts"]["events.jsonl"] = digest(events_path)
            report["plan"]["protocol"] = pilot.PROTOCOL
            report["preflight"] = {
                "cli": str(cli),
                "flagBridge": str(bridge),
                "flagBridgeSha256": digest(bridge),
                "flagParserSha256": digest(Path(cli_input.__file__)),
                "flagBridgeSourceSha256": digest(Path(cli_input._BRIDGE_SOURCE)),
                "runnerSha256": digest(Path(pilot.__file__)),
                "corpora": {"fixture": {"path": str(corpus)}},
            }
            write_json(root / "report.json", report)
            write_json(root / "manifest.json", {"report.json": digest(root / "report.json"),
                                                  bridge.name: digest(bridge)})
            actual = campaign_report.verify_campaign(root)
        self.assertEqual(actual["errors"], [], actual)
        self.assertTrue(actual["auditReplay"]["attempted"])


if __name__ == "__main__":
    unittest.main()
