"""Regression coverage for the benchmark's canonical CLI flag bridge."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

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
        with self.assertRaisesRegex(CliInputError, "Unknown localFetch flag"):
            parse_flag_query(self.bridge, "localFetch", ["--definitely-not-a-field"])


if __name__ == "__main__":
    unittest.main()
