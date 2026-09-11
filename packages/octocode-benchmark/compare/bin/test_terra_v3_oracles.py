#!/usr/bin/env python3
"""Tests for deterministic Terra v4 public-oracle materialization."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
SUITE = HERE.parent / "terra-v3" / "suite" / "public-cases.json"
sys.path.insert(0, str(HERE))


class TerraV3OracleTests(unittest.TestCase):
    def test_public_suite_has_supported_executable_anchors(self) -> None:
        from terra_v3_oracles import validate_suite_materializers

        document = json.loads(SUITE.read_text(encoding="utf-8"))
        self.assertEqual(document["version"], 4)
        self.assertEqual(validate_suite_materializers(document), [])
        self.assertEqual([case["id"][:3] for case in document["cases"]], [f"p{i:02}" for i in range(1, 21)])

    def test_materialization_binds_suite_corpus_tools_and_normalization(self) -> None:
        from terra_v3_oracles import materialize_public_oracles, validate_oracle_receipt

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            langchain = root / "langchain"
            nextjs = root / "nextjs"
            langchain.mkdir(); nextjs.mkdir()
            (langchain / "a.py").write_text("class RunnableConfig:\n    pass\n", encoding="utf-8")
            (nextjs / "a.ts").write_text("const NEXT_RUNTIME = 'nodejs'\n", encoding="utf-8")
            suite = {
                "version": 4,
                "split": "public",
                "normalizationVersion": "terra-public-oracle-v1",
                "cases": [{
                    "id": f"p{index:02}-test", "lane": "lexical", "repositories": ["langchain-ai/langchain"],
                    "prompt": "test", "anchor": {
                        "kind": "exact-result-set", "oracle": "python-regex",
                        "query": "RunnableConfig", "roots": ["."], "includeExtensions": [".py"],
                    },
                } for index in range(1, 21)],
            }
            commits = {
                "langchain-ai/langchain": "a" * 40,
                "vercel/next.js": "b" * 40,
            }
            receipt = materialize_public_oracles(
                suite, {"langchain-ai/langchain": langchain, "vercel/next.js": nextjs},
                commits=commits, corpus_receipt_digest="c" * 64,
            )
            self.assertEqual(receipt["status"], "complete")
            self.assertEqual(receipt["corpusReceiptDigest"], "c" * 64)
            self.assertRegex(receipt["suiteDigest"], r"^[0-9a-f]{64}$")
            self.assertRegex(receipt["normalizationReceipt"]["digest"], r"^[0-9a-f]{64}$")
            self.assertEqual(validate_oracle_receipt(receipt, suite), [])
            tampered = json.loads(json.dumps(receipt))
            tampered["cases"][0]["resultCount"] = 999
            self.assertTrue(validate_oracle_receipt(tampered, suite))

    def test_missing_specialist_is_a_typed_materialization_gap(self) -> None:
        from terra_v3_oracles import materialize_case

        case = {
            "id": "p09-test", "lane": "semantic", "repositories": ["langchain-ai/langchain"],
            "prompt": "test", "anchor": {
                "kind": "exact-result-set", "oracle": "direct-lsp",
                "server": "definitely-not-a-real-language-server", "request": {"method": "textDocument/definition"},
            },
        }
        result = materialize_case(case, {"langchain-ai/langchain": Path("/")}, tools={})
        self.assertEqual(result["status"], "gap")
        self.assertEqual(result["gap"]["code"], "tool-unavailable")
        self.assertEqual(result["gap"]["oracle"], "direct-lsp")

    def test_verified_fact_set_fails_closed_on_source_drift(self) -> None:
        from terra_v3_oracles import materialize_case

        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            (repo / "flow.py").write_text("def entry():\n    return helper()\n", encoding="utf-8")
            case = {
                "id": "p18-test", "lane": "end-to-end", "repositories": ["langchain-ai/langchain"],
                "prompt": "test", "anchor": {
                    "kind": "fact-set", "oracle": "verified-source-facts", "facts": [{
                        "id": "edge", "repository": "langchain-ai/langchain", "path": "flow.py",
                        "line": 2, "contains": "missing_helper()", "claim": "entry reaches helper",
                    }],
                },
            }
            result = materialize_case(case, {"langchain-ai/langchain": repo}, tools={})
            self.assertEqual(result["status"], "gap")
            self.assertEqual(result["gap"]["code"], "evidence-drift")

    def test_receipt_digest_is_canonical(self) -> None:
        from terra_v3_oracles import digest_record, seal_record

        record = seal_record({"b": 2, "a": 1})
        self.assertEqual(record["receiptDigest"], digest_record({"a": 1, "b": 2}))
        self.assertEqual(len(hashlib.sha256(b"x").hexdigest()), 64)


if __name__ == "__main__":
    unittest.main()
