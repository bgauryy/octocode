#!/usr/bin/env python3
"""Regression tests for the Headroom matchup measurement boundary."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
# Resolve the Headroom interpreter the same way the wrappers do (ghc/hr_compress):
# $HR_PY if set, else the uv-tool venv under the current user's home. Never hardcode
# an absolute path to one developer's machine — that silently breaks the Headroom
# measurement tests everywhere else.
HR_PY = Path(
    os.environ.get(
        "HR_PY", Path.home() / ".local/share/uv/tools/headroom-ai/bin/python"
    )
)


HR_AVAILABLE = HR_PY.is_file()
HR_REASON = f"Headroom interpreter not found at {HR_PY} (set $HR_PY)"


class InstrumentationTests(unittest.TestCase):
    def test_v3_strict_rejects_shared_cgroup_and_live_sampled_io(self) -> None:
        from instrument_command import _validate_v3

        record = {
            "corpus_digest": "a" * 64,
            "workspace_receipt_digest": "b" * 64,
            "fixture_manifest_digest": "c" * 64,
            "contracts_digest": "d" * 64,
            "logical_call_id": "case:arm:pass",
            "attempt_index": 1,
            "result_count": 1,
            "memory": {
                "peak_process_tree_rss_bytes": 1,
                "cgroup_v2": {"status": "shared", "isolated": False},
            },
            "cpu": {"user_ms": 1.0, "system_ms": 1.0, "complete_process_tree": False},
            "io": {"status": "sampled-live-incomplete", "complete_process_tree": False},
        }
        errors = _validate_v3(record, True, platform_name="linux")
        self.assertTrue(any("isolated Linux cgroup" in error for error in errors))
        self.assertTrue(any("complete process-tree I/O" in error for error in errors))
        self.assertTrue(any("complete process-tree CPU" in error for error in errors))

    def test_v3_strict_rejects_non_integer_or_negative_result_counts(self) -> None:
        from instrument_command import _validate_v3

        record = {
            "corpus_digest": "a" * 64, "workspace_receipt_digest": "b" * 64,
            "fixture_manifest_digest": "c" * 64, "contracts_digest": "d" * 64,
            "logical_call_id": "case:arm:pass", "attempt_index": 1, "exit_code": 0,
            "memory": {"peak_process_tree_rss_bytes": 1, "cgroup_v2": {"isolated": True}},
            "cpu": {"user_ms": 1.0, "system_ms": 1.0, "complete_process_tree": True},
            "io": {"complete_process_tree": True},
        }
        for count in (True, -1, None):
            with self.subTest(count=count):
                record["result_count"] = count
                self.assertTrue(any("non-negative integer" in error for error in
                                    _validate_v3(record, True, platform_name="linux")))

    def test_v3_outcome_is_derived_from_result_count(self) -> None:
        from instrument_command import _classify_outcome, _infer_result_count

        self.assertEqual(_infer_result_count(b'{"results":[{"path":"a"}]}'), 1)
        self.assertEqual(_infer_result_count(b'{"type":"match"}\n{"type":"summary"}\n'), 1)
        self.assertEqual(_classify_outcome(0, 1, None), "first-valid")
        self.assertEqual(_classify_outcome(0, 1, None, attempt_index=2), "productive-success")
        self.assertEqual(_classify_outcome(0, 0, "expected-absence"), "expected-empty")
        self.assertEqual(_classify_outcome(0, 0, "query-miss"), "unproductive-empty")
        self.assertEqual(_classify_outcome(2, None, None), "runtime-failed")
        self.assertEqual(_classify_outcome(2, None, None, raw=b"unknown field: directory"), "schema-invalid")

    @unittest.skipUnless(HR_AVAILABLE, HR_REASON)
    def test_headroom_record_preserves_transform_and_unicode_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log = tmp_path / "headroom.jsonl"
            artifacts = tmp_path / "artifacts"
            text = "café 🚀"
            env = {
                **os.environ,
                "HR_LOG": str(log),
                "HR_ARTIFACT_DIR": str(artifacts),
                "HR_COMMAND": "fixture:unicode",
            }

            result = subprocess.run(
                [str(HR_PY), str(HERE / "hr_compress.py")],
                input=text,
                text=True,
                capture_output=True,
                env=env,
                check=True,
            )

            self.assertEqual(result.stdout, text)
            record = json.loads(log.read_text(encoding="utf-8").strip())
            self.assertEqual(record["raw_chars"], len(text))
            self.assertEqual(record["out_chars"], len(result.stdout))
            self.assertEqual(record["char_unit"], "unicode_code_points")
            self.assertEqual(record["source_exit_code"], 0)
            self.assertIsInstance(record["transforms"], list)
            self.assertTrue(record["transforms"])
            self.assertEqual(Path(record["raw_artifact"]).read_text(encoding="utf-8"), text)
            self.assertEqual(
                Path(record["out_artifact"]).read_text(encoding="utf-8"), result.stdout
            )

    def test_generic_command_instrumentation_counts_unicode_and_keeps_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log = tmp_path / "octocode.jsonl"
            artifacts = tmp_path / "artifacts"
            text = "café 🚀"

            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "instrument_command.py"),
                    "--log",
                    str(log),
                    "--artifact-dir",
                    str(artifacts),
                    "--label",
                    "fixture:unicode",
                    "--",
                    sys.executable,
                    "-c",
                    f"print({text!r}, end='')",
                ],
                text=True,
                capture_output=True,
                check=True,
            )

            self.assertEqual(result.stdout, text)
            record = json.loads(log.read_text(encoding="utf-8").strip())
            self.assertEqual(record["model_in_chars"], len(text))
            self.assertNotIn("chars", record)
            self.assertEqual(record["char_unit"], "unicode_code_points")
            self.assertEqual(record["exit_code"], 0)
            self.assertEqual(Path(record["artifact"]).read_text(encoding="utf-8"), text)
            validation = subprocess.run(
                [sys.executable, str(HERE / "sumlog.py"), str(log), "--strict"],
                text=True, capture_output=True,
            )
            self.assertEqual(validation.returncode, 0, validation.stdout + validation.stderr)
            from validate_campaign import read_metrics
            self.assertEqual(read_metrics(log)["model_in_chars"], len(text))

    @unittest.skipUnless(HR_AVAILABLE, HR_REASON)
    def test_ghc_records_failed_probe_output_and_exit_status(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_gh = tmp_path / "gh"
            fake_gh.write_text("#!/usr/bin/env bash\nprintf 'remote query failed' >&2\nexit 7\n")
            fake_gh.chmod(0o755)
            log = tmp_path / "failed.jsonl"
            diagnostics = tmp_path / "failed-diagnostics.log"
            env = {
                **os.environ,
                "PATH": f"{tmp_path}:{os.environ['PATH']}",
                "HR_PY": str(HR_PY),
                "GHC_LOG": str(log),
                "GHC_ARTIFACT_DIR": str(tmp_path / "artifacts"),
                "GHC_DIAGNOSTICS_LOG": str(diagnostics),
            }

            result = subprocess.run(
                [str(HERE / "ghc"), "repo", "view", "owner/repo"],
                text=True,
                capture_output=True,
                env=env,
            )

            self.assertEqual(result.returncode, 7)
            record = json.loads(log.read_text(encoding="utf-8").strip())
            self.assertEqual(record["source_exit_code"], 7)
            self.assertEqual(
                Path(record["raw_artifact"]).read_text(encoding="utf-8"),
                "remote query failed",
            )
            validation = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "sumlog.py"),
                    str(log),
                    "--strict",
                    "--diagnostics",
                    str(diagnostics),
                ],
                text=True,
                capture_output=True,
            )
            self.assertEqual(validation.returncode, 0, validation.stdout + validation.stderr)
            self.assertIn("failed_calls=1", validation.stdout)

    def test_preflight_validator_self_test(self) -> None:
        result = subprocess.run(
            [sys.executable, str(HERE / "preflight.py"), "--self-test"],
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_v3_instrumentation_records_cpu_peak_rss_io_and_environment(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log = tmp_path / "v3.jsonl"
            artifacts = tmp_path / "artifacts"
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "instrument_command.py"),
                    "--log", str(log),
                    "--artifact-dir", str(artifacts),
                    "--label", "fixture:v3",
                    "--cache-cohort", "cold",
                    "--corpus-digest", "a" * 64,
                    "--workspace-receipt-digest", "b" * 64,
                    "--fixture-manifest-digest", "c" * 64,
                    "--contracts-digest", "d" * 64,
                    "--logical-call-id", "fixture:v3:1",
                    "--result-count", "1",
                    "--strict-v3",
                    "--",
                    sys.executable,
                    "-c",
                    "import pathlib; x=bytearray(8*1024*1024); pathlib.Path('/dev/null').write_bytes(x); print(len(x))",
                ],
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 125, result.stdout + result.stderr)
            record = json.loads(log.read_text(encoding="utf-8").strip())
            self.assertEqual(record["measurement_schema_version"], 3)
            self.assertEqual(record["cache_cohort"], "cold")
            self.assertGreater(record["wall_time_ms"], 0)
            self.assertGreaterEqual(record["cpu"]["user_ms"], 0)
            self.assertGreaterEqual(record["cpu"]["system_ms"], 0)
            self.assertGreater(record["memory"]["peak_process_tree_rss_bytes"], 0)
            self.assertIn(record["memory"]["cgroup_v2"]["status"], {"available", "unsupported"})
            self.assertGreaterEqual(record["io"]["read_bytes"], 0)
            self.assertGreaterEqual(record["io"]["write_bytes"], 0)
            self.assertEqual(record["corpus_digest"], "a" * 64)
            self.assertEqual(record["workspace_receipt_digest"], "b" * 64)
            self.assertRegex(record["environment_receipt_digest"], r"^[0-9a-f]{64}$")
            expected_error = "isolated Linux cgroup" if sys.platform.startswith("linux") else "requires Linux"
            self.assertTrue(any(expected_error in error for error in record["sensor_validation_errors"]))

    def test_v3_instrumentation_counts_child_memory_and_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log = tmp_path / "children.jsonl"
            child_code = "import time; x=bytearray(16*1024*1024); time.sleep(0.3)"
            parent_code = (
                "import subprocess,sys,time; "
                f"subprocess.Popen([sys.executable,'-c',{child_code!r}]); "
                f"subprocess.Popen([sys.executable,'-c',{child_code!r}]); time.sleep(2)"
            )
            result = subprocess.run(
                [
                    sys.executable, str(HERE / "instrument_command.py"),
                    "--log", str(log), "--artifact-dir", str(tmp_path / "artifacts"),
                    "--label", "fixture:children", "--cache-cohort", "warm-process",
                    "--timeout-seconds", "0.45", "--", sys.executable, "-c", parent_code,
                ],
                text=True, capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            record = json.loads(log.read_text(encoding="utf-8").strip())
            self.assertTrue(record["timed_out"])
            self.assertIsNotNone(record["signal"])
            self.assertGreater(record["memory"]["peak_process_tree_rss_bytes"], 28 * 1024 * 1024)

    def test_v3_strict_mode_rejects_missing_receipt_digest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [
                    sys.executable, str(HERE / "instrument_command.py"),
                    "--log", str(Path(tmp) / "invalid.jsonl"),
                    "--artifact-dir", str(Path(tmp) / "artifacts"),
                    "--label", "fixture:invalid", "--cache-cohort", "cold",
                    "--corpus-digest", "a" * 64,
                    "--strict-v3", "--", sys.executable, "-c", "print('x')",
                ],
                text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 125)
            record = json.loads((Path(tmp) / "invalid.jsonl").read_text(encoding="utf-8").strip())
            self.assertTrue(any("workspace_receipt_digest" in e for e in record["sensor_validation_errors"]))


if __name__ == "__main__":
    unittest.main()
