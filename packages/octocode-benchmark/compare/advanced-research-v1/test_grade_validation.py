"""Deterministic integrity checks for one blinded quality-review packet."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import grade_validation


def packet():
    return {
        "packetId": "local-03",
        "case": "A01",
        "outOf": 2,
        "answers": {
            "X": "The tag result is sorted. [config.py:10-12]",
            "Y": "The callback fails. [base.py:20]",
        },
    }


def grade(review_packet):
    return {
        "schemaVersion": 1,
        "packetId": review_packet["packetId"],
        "case": review_packet["case"],
        "packetSha256": grade_validation.sha256_json(review_packet),
        "answerSha256": {
            label: grade_validation.sha256_text(answer)
            for label, answer in review_packet["answers"].items()
        },
        "answers": {
            "X": {
                "outOf": 2,
                "factsCorrect": 1,
                "decisivelyCitedCorrect": 1,
                "unsupportedClaims": [],
                "atoms": [
                    {
                        "number": 1,
                        "behavior": "correct",
                        "citation": "decisive",
                        "explanation": "The result and citation are present.",
                        "sourceAnchors": ["answer: config.py:10-12"],
                        "answerQuotes": ["The tag result is sorted."],
                    },
                    {
                        "number": 2,
                        "behavior": "omitted",
                        "citation": "absent",
                        "explanation": "No callback claim appears.",
                        "sourceAnchors": [],
                        "answerQuotes": [],
                    },
                ],
            },
            "Y": {
                "outOf": 2,
                "factsCorrect": 0,
                "decisivelyCitedCorrect": 0,
                "unsupportedClaims": ["The callback fails."],
                "atoms": [
                    {
                        "number": 1,
                        "behavior": "omitted",
                        "citation": "absent",
                        "explanation": "No tag claim appears.",
                        "sourceAnchors": [],
                        "answerQuotes": [],
                    },
                    {
                        "number": 2,
                        "behavior": "contradicted",
                        "citation": "wrong",
                        "explanation": "The supplied source contradicts it.",
                        "sourceAnchors": ["answer: base.py:20"],
                        "answerQuotes": ["The callback fails."],
                    },
                ],
            },
        },
    }


class GradeValidationTests(unittest.TestCase):
    def setUp(self):
        self.packet = packet()
        self.grade = grade(self.packet)

    def test_valid_one_packet_review_passes(self):
        self.assertEqual(grade_validation.validate_review(self.packet, self.grade), [])

    def test_packet_and_each_answer_are_cryptographically_bound(self):
        changed_packet = copy.deepcopy(self.packet)
        changed_packet["answers"]["X"] += " changed"
        errors = grade_validation.validate_review(changed_packet, self.grade)
        self.assertTrue(any("packetSha256" in error for error in errors))
        self.assertTrue(any("answerSha256.X" in error for error in errors))

    def test_quote_must_be_exact_answer_text(self):
        self.grade["answers"]["X"]["atoms"][0]["answerQuotes"] = ["tag result sorted"]
        self.assertTrue(any("answerQuotes" in error for error in
                            grade_validation.validate_review(self.packet, self.grade)))

    def test_answer_anchor_range_must_be_cited_by_that_answer(self):
        self.grade["answers"]["X"]["atoms"][0]["sourceAnchors"] = ["answer: config.py:10-13"]
        self.assertTrue(any("not supported by an answer citation" in error for error in
                            grade_validation.validate_review(self.packet, self.grade)))

    def test_same_basename_in_another_directory_cannot_support_an_anchor(self):
        self.packet["answers"]["X"] = "The tag result is sorted. [src/a/config.py:10-12]"
        self.grade = grade(self.packet)
        self.grade["answers"]["X"]["atoms"][0]["sourceAnchors"] = ["answer: src/b/config.py:10-12"]
        self.assertTrue(any("not supported by an answer citation" in error for error in
                            grade_validation.validate_review(self.packet, self.grade)))

    def test_bare_filename_anchor_is_rejected_when_answer_cites_two_paths_with_that_name(self):
        self.packet["answers"]["X"] = (
            "The tag result is sorted. [src/a/config.py:10-12] [src/b/config.py:10-12]"
        )
        self.grade = grade(self.packet)
        self.assertTrue(any("not supported by an answer citation" in error for error in
                            grade_validation.validate_review(self.packet, self.grade)))

    def test_same_prefix_abbreviated_end_and_comma_ranges_are_expanded(self):
        citation = "[config.ts:1955-69, 292-304]"
        self.assertEqual(grade_validation._citations(citation), [
            ("config.ts", 1955, 1969),
            ("config.ts", 292, 304),
        ])
        self.packet["answers"]["X"] = "The tag result is sorted. " + citation
        self.grade = grade(self.packet)
        self.grade["answers"]["X"]["atoms"][0]["sourceAnchors"] = [
            "answer: config.ts:1955-1969"
        ]
        self.assertEqual(grade_validation.validate_review(self.packet, self.grade), [])

    def test_abbreviated_endpoint_that_wraps_is_not_guessed(self):
        self.assertEqual(grade_validation._citations("[config.ts:1998-02]"), [])

    def test_markdown_code_spans_preserve_explicit_comma_continuations(self):
        citation = "`src/config.py:142-147`, `271-291`, `419-427`"
        self.assertEqual(grade_validation._citations(citation), [
            ("src/config.py", 142, 147),
            ("src/config.py", 271, 291),
            ("src/config.py", 419, 427),
        ])
        self.assertTrue(grade_validation._anchor_is_cited(
            "answer:src/config.py:271-291", citation))
        self.assertFalse(grade_validation._anchor_is_cited(
            "answer:src/config.py:419-427", "`src/config.py:142-147`. Elsewhere `419-427`"))

    def test_colon_code_span_reuses_only_preceding_same_line_file(self):
        citation = "`src/a.py:10-12` returns; `:20-22` resets. `src/b.py:30`; `:40-41` ends."
        for anchor in ["answer:src/a.py:20-22", "answer:src/b.py:40-41"]:
            self.assertTrue(grade_validation._anchor_is_cited(anchor, citation))
        for anchor in ["answer:src/b.py:20-22", "answer:src/a.py:40-41"]:
            self.assertFalse(grade_validation._anchor_is_cited(anchor, citation))
        self.assertFalse(grade_validation._anchor_is_cited(
            "answer:src/a.py:20-22", "`src/a.py:10-12`\n\n`:20-22`"))

    def test_commit_metadata_citations_bind_full_identifiers_without_line_numbers(self):
        sha = "a" * 40
        url = f"https://github.com/example/repo/commit/{sha}"
        for anchor, answer in [(f"answer:commit:{sha}", f"Commit `{sha}`."),
                               (f"answer:{url}", f"[Commit metadata]({url})")]:
            self.assertTrue(grade_validation._anchor_is_cited(anchor, answer))
        self.assertFalse(grade_validation._anchor_is_cited(f"answer:commit:{sha}", sha[:12]))
        self.assertFalse(grade_validation._anchor_is_cited(f"answer:commit:{sha}", sha + "a"))
        self.assertFalse(grade_validation._anchor_is_cited(f"answer:{url}", url.replace("repo", "other")))

    def test_explicit_short_ranges_can_share_one_file_within_a_paragraph(self):
        self.assertTrue(grade_validation._anchor_is_cited(
            "answer:src/a.py:20-22", "- `src/a.py:10-12`\n- Reset: `:20-22`"))
        self.assertFalse(grade_validation._anchor_is_cited(
            "answer:src/a.py:20-22", "`src/a.py:10`\n`src/b.py:11`\n`:20-22`"))

    def test_typographic_line_range_separator_preserves_exact_bounds(self):
        self.assertTrue(grade_validation._anchor_is_cited(
            "answer:src/a.py:1955-1973", "`src/a.py:1955–1973`"))
        self.assertFalse(grade_validation._anchor_is_cited(
            "answer:src/a.py:1955-1974", "`src/a.py:1955–1973`"))

    def test_declared_sums_and_atom_numbers_must_match(self):
        self.grade["answers"]["Y"]["factsCorrect"] = 2
        self.grade["answers"]["Y"]["atoms"][1]["number"] = 1
        errors = grade_validation.validate_review(self.packet, self.grade)
        self.assertTrue(any("factsCorrect" in error for error in errors))
        self.assertTrue(any("numbers" in error for error in errors))

    def test_boolean_values_cannot_satisfy_integer_score_or_rubric_fields(self):
        self.packet["outOf"] = True
        self.grade = grade(self.packet)
        errors = grade_validation.validate_review(self.packet, self.grade)
        self.assertTrue(any("packet.outOf" in error for error in errors))

        self.packet = packet()
        self.grade = grade(self.packet)
        self.grade["answers"]["X"]["factsCorrect"] = True
        self.grade["answers"]["X"]["decisivelyCitedCorrect"] = True
        errors = grade_validation.validate_review(self.packet, self.grade)
        self.assertTrue(any("factsCorrect" in error for error in errors))
        self.assertTrue(any("decisivelyCitedCorrect" in error for error in errors))

    def test_boolean_values_cannot_satisfy_schema_or_per_answer_out_of(self):
        self.grade["schemaVersion"] = True
        self.grade["answers"]["X"]["outOf"] = True
        errors = grade_validation.validate_review(self.packet, self.grade)
        self.assertTrue(any("schemaVersion" in error for error in errors))
        self.assertTrue(any("answers.X.outOf" in error for error in errors))

    def test_unknown_top_level_payload_is_rejected_to_keep_one_packet_shape(self):
        self.grade["packets"] = [self.packet]
        self.assertTrue(any("unknown top-level" in error for error in
                            grade_validation.validate_review(self.packet, self.grade)))

    def test_malformed_json_values_return_errors_instead_of_crashing(self):
        atom = self.grade["answers"]["X"]["atoms"][0]
        self.grade["answers"]["X"]["atoms"][1]["number"] = "2"
        atom["behavior"] = {"correct": True}
        atom["citation"] = {"decisive": True}
        atom["answerQuotes"] = [{"not": "text"}]
        atom["sourceAnchors"] = [42]
        errors = grade_validation.validate_review(self.packet, self.grade)
        self.assertGreaterEqual(len(errors), 5)

    def test_validate_or_raise_makes_cli_callers_fail_closed(self):
        self.grade["case"] = "A02"
        with self.assertRaises(grade_validation.GradeValidationError):
            grade_validation.validate_or_raise(self.packet, self.grade)

    def test_cli_accepts_exactly_one_bound_packet_and_grade(self):
        with tempfile.TemporaryDirectory() as directory:
            packet_path = Path(directory) / "packet.json"
            grade_path = Path(directory) / "grade.json"
            packet_path.write_text(json.dumps(self.packet), encoding="utf-8")
            grade_path.write_text(json.dumps(self.grade), encoding="utf-8")
            self.assertEqual(grade_validation.main(["--packet", str(packet_path),
                                                    "--grade", str(grade_path)]), 0)
