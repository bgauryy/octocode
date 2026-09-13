"""Regression coverage for the benchmark's canonical CLI flag bridge."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
import shutil
import subprocess

from cli_input import CliInputError, parse_flag_query, prepare_bridge


class CliInputTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls._temporary_directory = tempfile.TemporaryDirectory()
        cls.bridge = prepare_bridge(Path(cls._temporary_directory.name))

    @classmethod
    def tearDownClass(cls) -> None:
        cls._temporary_directory.cleanup()

    def test_local_fetch_flags_match_the_canonical_cli_query(self) -> None:
        self.assertEqual(
            parse_flag_query(
                self.bridge,
                "localFetch",
                [
                    "--path",
                    "/fixture/source file.ts",
                    "--start-line",
                    "4",
                    "--endLine=8",
                    "--minify",
                    "none",
                ],
            ),
            {
                "path": "/fixture/source file.ts",
                "startLine": 4,
                "endLine": 8,
                "minify": "none",
            },
        )

    def test_local_fetch_flag_form_is_accepted_by_the_real_cli(self) -> None:
        node = shutil.which("node")
        self.assertIsNotNone(node)
        # The measured CLI only reads configured user-owned roots, so keep the
        # real-CLI fixture under the current user's home directory.
        with tempfile.TemporaryDirectory(dir=Path.home()) as directory:
            fixture = Path(directory) / "fixture.ts"
            fixture.write_text("fixture body\n", encoding="utf-8")
            cli = Path(__file__).resolve().parents[3] / "octocode" / "out" / "octocode.js"
            completed = subprocess.run(
                [
                    node,
                    str(cli),
                    "tools",
                    "localFetch",
                    "--path",
                    str(fixture),
                    "--start-line",
                    "1",
                    "--end-line=1",
                    "--minify",
                    "none",
                ],
                cwd=Path(__file__).resolve().parents[4],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(completed.returncode, 0, completed.stderr + completed.stdout)
        self.assertIn("fixture body", completed.stdout)

    def test_topology_selector_and_repeated_flags_are_preserved(self) -> None:
        self.assertEqual(
            parse_flag_query(
                self.bridge,
                "astSearch",
                [
                    "topology",
                    "path",
                    "--path",
                    "/fixture",
                    "--target",
                    "/fixture/target.ts",
                    "--exclude-dir",
                    "ignored",
                    "--excludeDir",
                    "coverage",
                ],
            ),
            {
                "operation": "topology",
                "analysis": "path",
                "path": "/fixture",
                "target": "/fixture/target.ts",
                "excludeDir": ["ignored", "coverage"],
            },
        )

    def test_outside_path_is_returned_for_policy_to_reject(self) -> None:
        self.assertEqual(
            parse_flag_query(
                self.bridge, "localFetch", ["--path", "/outside/secret.ts"]
            ),
            {"path": "/outside/secret.ts"},
        )

    def test_parser_error_is_explicit_and_never_executes_a_tool(self) -> None:
        with self.assertRaisesRegex(CliInputError, "Unknown localFetch flag") as caught:
            parse_flag_query(self.bridge, "localFetch", ["--definitely-not-a-field"])
        self.assertTrue(caught.exception.details)


if __name__ == "__main__":
    unittest.main()
