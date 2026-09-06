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


if __name__ == "__main__":
    unittest.main()
