#!/usr/bin/env python3
"""Deterministic and negative-control tests for the Terra v3 harness."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock


HERE = Path(__file__).resolve().parent
COMPARE = HERE.parent
sys.path.insert(0, str(HERE))


class TerraV3PreflightTests(unittest.TestCase):
    @staticmethod
    def _valid_campaign_contract() -> tuple[dict[str, object], list[dict[str, object]], dict[str, object], dict[str, object]]:
        arms = {
            "octocode": {"lanes": ["lexical"]},
            "ripgrep": {"lanes": ["lexical"]},
        }
        cases = [{"id": "p01", "lane": "lexical"}]
        roles = {
            "model": "gpt-5.6-terra",
            "roles": {
                role: {"reasoningEffort": "high"}
                for role in ("runner", "judge", "confirmationJudge")
            },
        }

        def token_usage() -> dict[str, object]:
            return {
                "source": "provider",
                "inputTokens": 100,
                "outputTokens": 20,
                "totalTokens": 120,
                "cachedInputTokens": 10,
                "reasoningTokens": 5,
            }

        role_receipts = []
        runs = []
        judges = []
        confirmations = []
        blind_assignments = []
        for pass_number in (1, 2, 3):
            for arm in arms:
                key = f"p01-{arm}-{pass_number}"
                records = [{
                    "logical_call_id": key,
                    "attempt_index": 1,
                    "exit_code": 0,
                    "result_count": 1,
                }]
                tool_receipt = {
                    "arm": arm,
                    "argv0": f"/opt/tools/{arm}",
                    "executableDigest": "c" * 64,
                    "versionCommand": [arm, "--version"],
                    "versionOutput": "1.0.0",
                }
                tool_receipt["receiptDigest"] = hashlib.sha256(
                    json.dumps(tool_receipt, sort_keys=True, separators=(",", ":")).encode()
                ).hexdigest()
                runs.append({
                    "caseId": "p01",
                    "arm": arm,
                    "pass": pass_number,
                    "answerDigest": hashlib.sha256(key.encode()).hexdigest(),
                    "workspaceReceiptDigest": "a" * 64,
                    "corpusLockDigest": "b" * 64,
                    "toolReceipt": tool_receipt,
                    "tokenUsage": token_usage(),
                    "metrics": {
                        "wallMs": 10,
                        "cpuUserMs": 4,
                        "cpuSystemMs": 1,
                        "peakRssBytes": 1024,
                        "cgroupPeakBytes": 2048,
                        "ioReadBytes": 100,
                        "ioWriteBytes": 20,
                        "contextCharacters": 300,
                    },
                    "anchorResult": {
                        "passed": True,
                        "kind": "exact-result-set",
                        "expectedDigest": "e" * 64,
                        "actualDigest": "f" * 64,
                    },
                    "trajectory": {
                        "records": records,
                        "summary": {
                            "totalCalls": 1,
                            "firstValidCalls": 1,
                            "schemaInvalidCalls": 0,
                            "runtimeFailedCalls": 0,
                            "productiveSuccessCalls": 0,
                            "expectedEmptyCalls": 0,
                            "unproductiveEmptyCalls": 0,
                            "repairedSchemaCalls": 0,
                            "schemaInvalidRate": 0.0,
                            "unproductiveEmptyRate": 0.0,
                        },
                    },
                })
                role_receipts.append({
                    "caseId": "p01", "arm": arm, "pass": pass_number,
                    "role": "runner", "model": "gpt-5.6-terra",
                    "reasoningEffort": "high", "contextId": f"runner-{key}",
                    "tokenUsage": token_usage(),
                })
            for role, collection in (("judge", judges), ("confirmationJudge", confirmations)):
                judge_id = f"{role}-p01-ripgrep-{pass_number}"
                record = {
                    "judgeId": judge_id,
                    "caseId": "p01", "arm": "ripgrep", "pass": pass_number,
                    "labels": ["X", "Y"], "orderHidden": True,
                    "reasonedBeforeScores": True,
                    "scores": {
                        label: {"correctness": 10, "depth": 5, "workflow": 5}
                        for label in ("X", "Y")
                    },
                    "anchorResult": {
                        "passed": True,
                        "kind": "exact-result-set",
                        "expectedDigest": "e" * 64,
                        "actualDigest": "f" * 64,
                    },
                }
                if role == "confirmationJudge":
                    record["orderReversed"] = True
                collection.append(record)
                role_receipts.append({
                    "judgeId": judge_id,
                    "caseId": "p01", "arm": "ripgrep", "pass": pass_number,
                    "role": role, "model": "gpt-5.6-terra",
                    "reasoningEffort": "high", "contextId": judge_id,
                    "tokenUsage": token_usage(),
                })
                assignment = {
                    "judgeId": judge_id,
                    "caseId": "p01", "arm": "ripgrep", "pass": pass_number,
                }
                label_to_arm = (
                    {"X": "ripgrep", "Y": "octocode"}
                    if role == "confirmationJudge"
                    else {"X": "octocode", "Y": "ripgrep"}
                )
                assignment["labelToArm"] = label_to_arm
                assignment["answerDigests"] = {
                    label: hashlib.sha256(f"p01-{assigned_arm}-{pass_number}".encode()).hexdigest()
                    for label, assigned_arm in label_to_arm.items()
                }
                assignment["assignmentDigest"] = hashlib.sha256(
                    json.dumps(assignment, sort_keys=True, separators=(",", ":")).encode()
                ).hexdigest()
                blind_assignments.append(assignment)
        campaign = {
            "publicOracleReceiptDigest": "d" * 64,
            "runs": runs,
            "judges": judges,
            "confirmationJudges": confirmations,
            "roleReceipts": role_receipts,
            "blindAssignments": blind_assignments,
        }
        return campaign, cases, arms, roles

    def test_arm_reverifies_source_paths_recorded_in_corpus_lock(self) -> None:
        from terra_v3_arm import verify_locked_corpus
        from terra_v3_preflight import build_corpus_lock

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths = {}
            commits = {}
            for index, repo in enumerate(("langchain-ai/langchain", "vercel/next.js")):
                path = root / str(index)
                path.mkdir()
                subprocess.run(["git", "init", "-q"], cwd=path, check=True)
                subprocess.run(["git", "config", "user.name", "Fixture"], cwd=path, check=True)
                subprocess.run(["git", "config", "user.email", "fixture@example.invalid"], cwd=path, check=True)
                (path / "source.py").write_text(f"value={index}\n", encoding="utf-8")
                subprocess.run(["git", "add", "."], cwd=path, check=True)
                subprocess.run(["git", "commit", "-qm", "fixture"], cwd=path, check=True)
                paths[repo] = path
                commits[repo] = subprocess.run(
                    ["git", "rev-parse", "HEAD"], cwd=path, text=True, capture_output=True, check=True
                ).stdout.strip()
            lock = build_corpus_lock(None, "f" * 64, repo_paths=paths, expected_commits=commits)
            self.assertEqual(verify_locked_corpus(lock), [])
            (paths["langchain-ai/langchain"] / "source.py").write_text("tampered\n", encoding="utf-8")
            self.assertTrue(any("content digest drift" in error for error in verify_locked_corpus(lock)))

    def test_two_clone_lock_is_repeatable_and_tamper_evident(self) -> None:
        from terra_v3_preflight import build_corpus_lock, digest_record, verify_corpus_bytes

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths = {
                "langchain-ai/langchain": root / "langchain",
                "vercel/next.js": root / "nextjs",
            }
            commits = {}
            for index, (repo, path) in enumerate(paths.items()):
                path.mkdir()
                subprocess.run(["git", "init", "-q"], cwd=path, check=True)
                subprocess.run(["git", "config", "user.name", "Benchmark Fixture"], cwd=path, check=True)
                subprocess.run(["git", "config", "user.email", "fixture@example.invalid"], cwd=path, check=True)
                suffix = ".py" if index == 0 else ".ts"
                (path / f"fixture{suffix}").write_text(f"value = {index}\n", encoding="utf-8")
                subprocess.run(["git", "add", "."], cwd=path, check=True)
                subprocess.run(["git", "commit", "-qm", "fixture"], cwd=path, check=True)
                commits[repo] = subprocess.run(
                    ["git", "rev-parse", "HEAD"], cwd=path, text=True, capture_output=True, check=True
                ).stdout.strip()
            first = build_corpus_lock(None, "1" * 64, repo_paths=paths, expected_commits=commits)
            second = build_corpus_lock(None, "1" * 64, repo_paths=paths, expected_commits=commits)
            self.assertEqual(digest_record(first), digest_record(second))
            self.assertEqual(verify_corpus_bytes(first, None, repo_paths=paths), [])
            (paths["langchain-ai/langchain"] / "fixture.py").write_text("tampered\n", encoding="utf-8")
            self.assertTrue(any("content digest drift" in e for e in verify_corpus_bytes(first, None, repo_paths=paths)))

    def test_checked_in_public_oracles_bind_the_frozen_commits(self) -> None:
        from terra_v3_preflight import DEFAULT_PUBLIC_ORACLES, FROZEN_COMMITS, frozen_oracle_commits

        self.assertEqual(frozen_oracle_commits(DEFAULT_PUBLIC_ORACLES), FROZEN_COMMITS)
        self.assertEqual(frozen_oracle_commits(Path("/definitely/missing/oracles.json")), FROZEN_COMMITS)

    def test_public_oracle_commit_mismatch_fails_closed(self) -> None:
        from terra_v3_preflight import FROZEN_COMMITS, PreflightError, frozen_oracle_commits

        with tempfile.TemporaryDirectory() as tmp:
            receipt = Path(tmp) / "public-oracles.json"
            repositories = [
                {"repo": repo, "commit": ("0" * 40 if index == 0 else commit)}
                for index, (repo, commit) in enumerate(FROZEN_COMMITS.items())
            ]
            receipt.write_text(json.dumps({"repositories": repositories}), encoding="utf-8")
            with self.assertRaisesRegex(PreflightError, "differ from the frozen Terra v4 corpus"):
                frozen_oracle_commits(receipt)

    def test_prepare_uses_frozen_commits_not_moving_remote_heads(self) -> None:
        import terra_v3_preflight as preflight

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            specs = []
            frozen = {}
            for index, original in enumerate(preflight.REPOSITORIES):
                remote = root / f"remote-{index}"
                remote.mkdir()
                subprocess.run(["git", "init", "-q", "-b", original["ref"]], cwd=remote, check=True)
                subprocess.run(["git", "config", "user.name", "Fixture"], cwd=remote, check=True)
                subprocess.run(["git", "config", "user.email", "fixture@example.invalid"], cwd=remote, check=True)
                source = remote / "source.txt"
                source.write_text("frozen\n", encoding="utf-8")
                subprocess.run(["git", "add", "source.txt"], cwd=remote, check=True)
                subprocess.run(["git", "commit", "-qm", "frozen"], cwd=remote, check=True)
                frozen[original["repo"]] = subprocess.run(
                    ["git", "rev-parse", "HEAD"], cwd=remote, text=True, capture_output=True, check=True
                ).stdout.strip()
                source.write_text("moving head\n", encoding="utf-8")
                subprocess.run(["git", "commit", "-qam", "moving head"], cwd=remote, check=True)
                specs.append({**original, "url": str(remote)})

            corpus = root / "corpus"
            with mock.patch.object(preflight, "REPOSITORIES", tuple(specs)):
                preflight.prepare_repositories(corpus, expected_commits=frozen)
            for spec in specs:
                checkout = corpus / spec["directory"]
                actual = subprocess.run(
                    ["git", "rev-parse", "HEAD"], cwd=checkout, text=True, capture_output=True, check=True
                ).stdout.strip()
                self.assertEqual(actual, frozen[spec["repo"]])
                self.assertEqual((checkout / "source.txt").read_text(encoding="utf-8"), "frozen\n")

    def test_corpus_lock_requires_exactly_the_two_approved_repositories(self) -> None:
        from terra_v3_preflight import validate_corpus_lock

        valid = {
            "version": 3,
            "repositories": [
                {
                    "repo": "langchain-ai/langchain",
                    "ref": "master",
                    "commit": "a" * 40,
                    "tree": "b" * 40,
                    "contentDigest": "c" * 64,
                    "fileCount": 1,
                    "sourceBytes": 7,
                    "languages": {"Python": {"files": 1, "bytes": 7}},
                },
                {
                    "repo": "vercel/next.js",
                    "ref": "canary",
                    "commit": "d" * 40,
                    "tree": "e" * 40,
                    "contentDigest": "f" * 64,
                    "fileCount": 1,
                    "sourceBytes": 9,
                    "languages": {"TypeScript": {"files": 1, "bytes": 9}},
                },
            ],
            "exclusions": [".git", "node_modules", "dist", "build", "out", "target", ".next"],
            "controlExclusions": [".octocode-clone-meta.json"],
            "fixtureManifestDigest": "1" * 64,
        }
        self.assertEqual(validate_corpus_lock(valid), [])
        tampered = json.loads(json.dumps(valid))
        tampered["repositories"][1]["repo"] = "someone/small-repo"
        self.assertTrue(any("repository set" in item for item in validate_corpus_lock(tampered)))
        duplicate = json.loads(json.dumps(valid))
        duplicate["repositories"][1] = duplicate["repositories"][0]
        self.assertTrue(validate_corpus_lock(duplicate))

    def test_content_digest_is_stable_and_detects_tampering(self) -> None:
        from terra_v3_preflight import hash_content_tree

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "src").mkdir()
            (root / "src" / "a.py").write_text("print('a')\n", encoding="utf-8")
            (root / "node_modules").mkdir()
            (root / "node_modules" / "ignored.js").write_text("one", encoding="utf-8")
            first = hash_content_tree(root)
            (root / "node_modules" / "ignored.js").write_text("two", encoding="utf-8")
            self.assertEqual(hash_content_tree(root), first)
            (root / "src" / "a.py").write_text("print('b')\n", encoding="utf-8")
            self.assertNotEqual(hash_content_tree(root)["contentDigest"], first["contentDigest"])

    def test_octocode_invocation_rejects_npx_global_and_out_of_root_scripts(self) -> None:
        from terra_v3_preflight import validate_octocode_argv

        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            cli = workspace / "packages/octocode/out/octocode.js"
            cli.parent.mkdir(parents=True)
            cli.write_text("", encoding="utf-8")
            node = Path(shutil.which("node") or "node")
            self.assertEqual(validate_octocode_argv([str(node), str(cli), "tools"], workspace), [])
            self.assertTrue(validate_octocode_argv(["npx", "octocode", "tools"], workspace))
            self.assertTrue(validate_octocode_argv(["octocode", "tools"], workspace))
            outside = workspace.parent / "outside-octocode.js"
            outside.write_text("", encoding="utf-8")
            try:
                self.assertTrue(validate_octocode_argv([str(node), str(outside)], workspace))
            finally:
                outside.unlink()

    def test_workspace_receipt_rejects_stale_or_tampered_artifacts(self) -> None:
        from terra_v3_preflight import build_workspace_receipt, validate_workspace_receipt

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "packages/octocode/src/main.ts"
            cli = root / "packages/octocode/out/octocode.js"
            native = root / "packages/octocode-engine/octocode-engine.test.node"
            lock = root / "yarn.lock"
            for path, content in ((source, "source"), (cli, "cli"), (native, "native"), (lock, "lock")):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")
            now = time.time()
            os.utime(source, (now - 10, now - 10))
            os.utime(cli, (now, now))
            os.utime(native, (now, now))
            receipt = build_workspace_receipt(root, catalog_bytes=b"catalog")
            self.assertEqual(validate_workspace_receipt(receipt, root), [])
            cli.write_text("tampered", encoding="utf-8")
            self.assertTrue(any("cli artifact digest" in e for e in validate_workspace_receipt(receipt, root)))
            os.utime(source, (now + 20, now + 20))
            self.assertTrue(any("stale" in e for e in validate_workspace_receipt(receipt, root)))

    def test_workspace_receipt_binds_config_and_sibling_core_sources(self) -> None:
        from terra_v3_preflight import build_workspace_receipt, validate_workspace_receipt

        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            root = parent / "octocode"
            core = parent / "octocode-mcp-host/packages/octocode-core/src/schema.ts"
            files = {
                root / "packages/octocode-config/src/index.ts": "config",
                root / "packages/octocode/src/main.ts": "cli-source",
                root / "packages/octocode/out/octocode.js": "cli",
                root / "packages/octocode-engine/native.node": "native",
                root / "yarn.lock": "lock",
                core: "core",
            }
            for path, content in files.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")
            now = time.time()
            for path in files:
                os.utime(path, (now - 1, now - 1))
            os.utime(root / "packages/octocode/out/octocode.js", (now, now))
            os.utime(root / "packages/octocode-engine/native.node", (now, now))
            receipt = build_workspace_receipt(root, catalog_bytes=b"catalog")
            self.assertEqual(validate_workspace_receipt(receipt, root), [])
            core.write_text("changed", encoding="utf-8")
            self.assertTrue(any("external source" in e for e in validate_workspace_receipt(receipt, root)))

    def test_canonical_core_receipt_rejects_published_stale_and_export_drift(self) -> None:
        from terra_v3_preflight import build_canonical_core_receipt, validate_canonical_core_receipt

        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            workspace = parent / "octocode"
            canonical = parent / "octocode-mcp-host/packages/octocode-core"
            installed = workspace / "node_modules/@octocodeai/octocode-core"
            for root in (canonical, installed):
                (root / "src").mkdir(parents=True)
                (root / "dist").mkdir()
                (root / "src/schema.ts").write_text("export const schema = 1\n", encoding="utf-8")
                (root / "dist/schema.js").write_text("export const schema = 1\n", encoding="utf-8")
                (root / "dist/mcp.js").write_text("export const mcp = 1\n", encoding="utf-8")
                (root / "dist/index.js").write_text("export const index = 1\n", encoding="utf-8")
                (root / "package.json").write_text(json.dumps({
                    "name": "@octocodeai/octocode-core", "version": "1.0.0", "type": "module",
                    "exports": {".": {"import": "./dist/index.js"}, "./schema": {"import": "./dist/schema.js"}, "./mcp": {"import": "./dist/mcp.js"}},
                }), encoding="utf-8")
            workspace.mkdir(exist_ok=True)
            (workspace / "package.json").write_text(json.dumps({
                "resolutions": {"@octocodeai/octocode-core": canonical.as_uri()}
            }), encoding="utf-8")
            now = time.time()
            os.utime(canonical / "src/schema.ts", (now - 10, now - 10))
            for path in (canonical / "dist").iterdir():
                os.utime(path, (now, now))
            receipt = build_canonical_core_receipt(workspace)
            self.assertEqual(validate_canonical_core_receipt(receipt, workspace), [])

            published = json.loads((workspace / "package.json").read_text())
            published["resolutions"]["@octocodeai/octocode-core"] = "19.0.2"
            (workspace / "package.json").write_text(json.dumps(published), encoding="utf-8")
            self.assertTrue(any("local file" in error for error in validate_canonical_core_receipt(receipt, workspace)))
            published["resolutions"]["@octocodeai/octocode-core"] = canonical.as_uri()
            (workspace / "package.json").write_text(json.dumps(published), encoding="utf-8")

            installed_schema = installed / "dist/schema.js"
            installed_schema.write_text("export const schema = 2\n", encoding="utf-8")
            self.assertTrue(any("export fingerprint" in error for error in validate_canonical_core_receipt(receipt, workspace)))
            installed_schema.write_text("export const schema = 1\n", encoding="utf-8")
            os.utime(canonical / "src/schema.ts", (now + 20, now + 20))
            self.assertTrue(any("stale canonical core" in error for error in validate_canonical_core_receipt(receipt, workspace)))


class TerraV3ContractTests(unittest.TestCase):
    _valid_campaign_contract = staticmethod(TerraV3PreflightTests._valid_campaign_contract)

    def test_direct_lsp_arm_wraps_server_with_real_client(self) -> None:
        from terra_v3_arm import build_measured_argv

        root = Path("/fixture").resolve()
        argv = build_measured_argv(
            "lsp-pyright", ["pyright-langserver", "--stdio"], root,
            lsp_root=root, lsp_method="textDocument/references",
            lsp_params_json='{"textDocument":{"uri":"file:///x.py"}}',
        )
        self.assertEqual(Path(argv[1]).name, "terra_v3_lsp_client.py")
        self.assertIn("pyright-langserver", argv)
        self.assertIn("textDocument/references", argv)

    def test_language_server_receipt_replays_exact_resolved_command_and_fingerprints(self) -> None:
        from terra_v3_lsp_receipt import (
            build_language_server_receipt,
            validate_language_server_receipt,
            validate_lsp_replay,
        )

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            package = root / "node_modules/pyright"
            executable = package / "bin/pyright-langserver"
            executable.parent.mkdir(parents=True)
            executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            executable.chmod(0o755)
            (package / "package.json").write_text(json.dumps({"name": "pyright", "version": "1.2.3"}), encoding="utf-8")
            config = root / "pyrightconfig.json"
            config.write_text('{"typeCheckingMode":"basic"}\n', encoding="utf-8")
            probe = {
                "initialize": {"result": {"capabilities": {"definitionProvider": True}}},
                "response": {"result": []}, "shutdown": {"result": None},
            }
            receipt = build_language_server_receipt(
                "lsp-pyright", [str(executable), "--stdio"], root,
                config_paths=[config], initialization_options={"pythonPath": "/frozen/python"},
                probe=probe,
            )
            self.assertEqual(validate_language_server_receipt(receipt, "lsp-pyright", root), [])
            self.assertEqual(validate_lsp_replay(receipt, receipt["resolvedCommand"], root), [])
            self.assertTrue(validate_lsp_replay(receipt, [str(executable), "--stdio", "--changed"], root))
            config.write_text('{"typeCheckingMode":"strict"}\n', encoding="utf-8")
            self.assertTrue(any("config fingerprint" in error for error in validate_language_server_receipt(receipt, "lsp-pyright", root)))
            config.write_text('{"typeCheckingMode":"basic"}\n', encoding="utf-8")
            executable.write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
            self.assertTrue(any("executable digest" in error for error in validate_language_server_receipt(receipt, "lsp-pyright", root)))

    def test_language_server_receipt_rejects_unready_or_wrong_workspace(self) -> None:
        from terra_v3_lsp_receipt import build_language_server_receipt, validate_language_server_receipt

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            executable = root / "typescript-language-server"
            executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            executable.chmod(0o755)
            (root / "package.json").write_text(json.dumps({"name": "typescript-language-server", "version": "4.0.0"}), encoding="utf-8")
            probe = {"initialize": {"result": {"capabilities": {}}}, "response": {"error": {"code": -1}}, "shutdown": {"result": None}}
            receipt = build_language_server_receipt(
                "lsp-typescript", [str(executable), "--stdio"], root,
                config_paths=[], initialization_options={}, probe=probe,
            )
            self.assertTrue(validate_language_server_receipt(receipt, "lsp-typescript", root))
            ready = json.loads(json.dumps(receipt))
            ready["readiness"]["probeSucceeded"] = True
            self.assertTrue(validate_language_server_receipt(ready, "lsp-typescript", root / "other"))

    def test_direct_lsp_client_initializes_requests_and_shuts_down(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            server = root / "fake_lsp.py"
            server.write_text(
                "import json,sys\n"
                "def read():\n"
                "  headers={}\n"
                "  while True:\n"
                "    line=sys.stdin.buffer.readline()\n"
                "    if line in (b'\\r\\n',b'\\n',b''): break\n"
                "    k,v=line.decode().split(':',1); headers[k.lower()]=v.strip()\n"
                "  return json.loads(sys.stdin.buffer.read(int(headers['content-length'])))\n"
                "def send(x):\n"
                "  b=json.dumps(x,separators=(',',':')).encode(); sys.stdout.buffer.write(f'Content-Length: {len(b)}\\r\\n\\r\\n'.encode()+b); sys.stdout.buffer.flush()\n"
                "while True:\n"
                "  m=read()\n"
                "  if m.get('method')=='initialize': send({'jsonrpc':'2.0','id':m['id'],'result':{'capabilities':{}}})\n"
                "  elif m.get('method')=='fixture/query': send({'jsonrpc':'2.0','id':m['id'],'result':[{'uri':'file:///fixture.py'}]})\n"
                "  elif m.get('method')=='shutdown': send({'jsonrpc':'2.0','id':m['id'],'result':None})\n"
                "  elif m.get('method')=='exit': break\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable, str(HERE / "terra_v3_lsp_client.py"),
                    "--root", str(root), "--method", "fixture/query", "--params-json", "{}",
                    "--server", sys.executable, str(server),
                ],
                text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            payload = json.loads(result.stdout)
            self.assertEqual(payload["response"]["result"], [{"uri": "file:///fixture.py"}])

    def test_trajectory_classifier_preserves_schema_retry_and_empty_predecessors(self) -> None:
        from terra_v3_trajectory import classify_call, summarize_trajectory

        records = [
            {"logical_call_id": "a", "attempt_index": 1, "exit_code": 2, "output": {"error": "Unknown field: directory"}},
            {"logical_call_id": "a", "attempt_index": 2, "exit_code": 0, "result_count": 3},
            {"logical_call_id": "b", "attempt_index": 1, "exit_code": 0, "result_count": 0, "empty_classification": "query-miss"},
            {"logical_call_id": "c", "attempt_index": 1, "exit_code": 0, "result_count": 0, "empty_classification": "expected-absence"},
        ]
        self.assertEqual(classify_call(records[0]), "schema-invalid")
        summary = summarize_trajectory(records)
        self.assertEqual(summary["schemaInvalidCalls"], 1)
        self.assertEqual(summary["repairedSchemaCalls"], 1)
        self.assertEqual(summary["unproductiveEmptyCalls"], 1)
        self.assertEqual(summary["expectedEmptyCalls"], 1)
        self.assertEqual(summary["totalCalls"], 4)

    def test_checked_in_contracts_freeze_fair_arms_and_terra_roles(self) -> None:
        from terra_v3_contracts import load_and_validate_contracts

        bundle, errors = load_and_validate_contracts(COMPARE / "terra-v3/contracts")
        self.assertEqual(errors, [])
        self.assertEqual(bundle["roles"]["model"], "gpt-5.6-terra")
        self.assertEqual(set(bundle["roles"]["roles"]), {"runner", "judge", "confirmationJudge"})
        self.assertEqual(
            set(bundle["arms"]),
            {"octocode", "ripgrep", "ast-grep", "lsp-pyright", "lsp-typescript", "sourcegraph"},
        )

    def test_deterministic_grader_rejects_wrong_sets_and_wrong_patch_bytes(self) -> None:
        from terra_v3_contracts import grade_deterministic

        expected = [{"path": "a.py", "line": 2}, {"path": "b.py", "line": 7}]
        self.assertTrue(grade_deterministic(expected, list(reversed(expected)), "exact-result-set")["passed"])
        self.assertFalse(grade_deterministic(expected, expected[:1], "exact-result-set")["passed"])
        patch = "--- a/a.py\n+++ b/a.py\n"
        digest = hashlib.sha256(patch.encode()).hexdigest()
        self.assertTrue(grade_deterministic(digest, patch, "patch-hash")["passed"])
        self.assertFalse(grade_deterministic(digest, patch + "x", "patch-hash")["passed"])

    def test_arm_validator_rejects_emulation_and_wrong_octocode_entrypoint(self) -> None:
        from terra_v3_contracts import validate_arm_argv

        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            cli = workspace / "packages/octocode/out/octocode.js"
            cli.parent.mkdir(parents=True)
            cli.write_text("", encoding="utf-8")
            self.assertTrue(validate_arm_argv("ripgrep", ["python", "fake_rg.py"], workspace))
            self.assertTrue(validate_arm_argv("octocode", ["npx", "octocode"], workspace))
            self.assertEqual(
                validate_arm_argv("sourcegraph", ["src", "search", "-json", "-stream", "repo:x"], workspace),
                [],
            )

    def test_suite_contract_has_twenty_public_and_twenty_hash_only_private_cases(self) -> None:
        from terra_v3_contracts import validate_suite_contract

        errors, summary = validate_suite_contract(COMPARE / "terra-v3/suite")
        self.assertEqual(errors, [])
        self.assertEqual(summary["publicCount"], 20)
        self.assertEqual(summary["privateCount"], 20)
        self.assertFalse(summary["publicOraclesReady"])
        self.assertFalse(summary["campaignReady"])
        private_manifest = json.loads(
            (COMPARE / "terra-v3/suite/private-manifest.json").read_text(encoding="utf-8")
        )
        self.assertTrue(all("prompt" not in case and "answer" not in case for case in private_manifest["cases"]))

    def test_public_oracle_gate_validates_complete_hash_chain(self) -> None:
        from terra_v3_contracts import _validate_public_oracle_receipt, digest_record

        suite = {
            "normalizationVersion": "terra-public-oracle-v1",
            "cases": [{
                "id": "p01", "anchor": {"kind": "exact-result-set", "oracle": "python-regex"},
            }],
        }
        answer = [{"path": "a.py", "line": 1}]
        case_receipt = {
            "caseId": "p01", "status": "materialized", "kind": "exact-result-set",
            "oracle": "python-regex", "resultCount": 1, "answer": answer,
            "answerDigest": digest_record(answer),
        }
        case_receipt["receiptDigest"] = digest_record(case_receipt)
        normalization = {
            "version": "terra-public-oracle-v1", "paths": "repository-relative-posix",
            "lines": "one-based", "columns": "zero-based-unicode-scalar",
            "ordering": "canonical-json-lexicographic", "patch": "unified-diff-a-b-prefix-lf",
        }
        normalization_digest = digest_record(normalization)
        normalization.update({"receiptDigest": normalization_digest, "digest": normalization_digest})
        receipt = {
            "version": 4, "status": "complete", "suiteDigest": digest_record(suite),
            "corpusReceiptDigest": "b" * 64,
            "repositories": [
                {"repo": "langchain-ai/langchain", "commit": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125"},
                {"repo": "vercel/next.js", "commit": "d155ba9ebfffe4742efefda8d68c2e0e8e490924"},
            ],
            "normalizationReceipt": normalization, "toolReceipts": [], "cases": [case_receipt],
        }
        receipt["receiptDigest"] = digest_record(receipt)
        errors, summary = _validate_public_oracle_receipt(receipt, suite)
        self.assertEqual(errors, [])
        self.assertTrue(summary["ready"])

        mutations = []
        for field, value in (
            ("suiteDigest", "0" * 64), ("corpusReceiptDigest", "invalid"),
        ):
            invalid = json.loads(json.dumps(receipt))
            invalid[field] = value
            invalid["receiptDigest"] = digest_record({
                key: item for key, item in invalid.items() if key != "receiptDigest"
            })
            mutations.append(invalid)
        wrong_repo = json.loads(json.dumps(receipt))
        wrong_repo["repositories"][0]["commit"] = "0" * 40
        wrong_repo["receiptDigest"] = digest_record({
            key: item for key, item in wrong_repo.items() if key != "receiptDigest"
        })
        mutations.append(wrong_repo)
        tampered_answer = json.loads(json.dumps(receipt))
        tampered_answer["cases"][0]["answer"][0]["line"] = 2
        tampered_answer["receiptDigest"] = digest_record({
            key: item for key, item in tampered_answer.items() if key != "receiptDigest"
        })
        mutations.append(tampered_answer)
        incomplete = json.loads(json.dumps(receipt))
        incomplete["status"] = "incomplete"
        incomplete["cases"] = [{
            "caseId": "p01", "status": "gap",
            "gap": {"code": "tool-unavailable", "oracle": "direct-lsp", "detail": "missing"},
        }]
        incomplete["receiptDigest"] = digest_record({
            key: item for key, item in incomplete.items() if key != "receiptDigest"
        })
        incomplete_errors, incomplete_summary = _validate_public_oracle_receipt(incomplete, suite)
        self.assertEqual(incomplete_errors, [])
        self.assertFalse(incomplete_summary["ready"])
        for invalid in mutations:
            invalid_errors, invalid_summary = _validate_public_oracle_receipt(invalid, suite)
            self.assertTrue(invalid_errors)
            self.assertFalse(invalid_summary["ready"])

    def test_role_receipt_rejects_mixed_models_context_reuse_and_arm_effort_drift(self) -> None:
        from terra_v3_contracts import validate_role_receipts

        receipts = [
            {
                "caseId": "p01", "arm": arm, "pass": 1, "role": "runner",
                "model": "gpt-5.6-terra", "reasoningEffort": "high", "contextId": f"ctx-{arm}",
                "tokenUsage": {"source": "provider", "inputTokens": 10, "outputTokens": 2, "totalTokens": 12},
            }
            for arm in ("octocode", "ripgrep")
        ]
        self.assertEqual(validate_role_receipts(receipts), [])
        mixed = [dict(receipts[0]), {**receipts[1], "model": "gpt-5.5"}]
        self.assertTrue(validate_role_receipts(mixed))
        reused = [dict(receipts[0]), {**receipts[1], "contextId": receipts[0]["contextId"]}]
        self.assertTrue(validate_role_receipts(reused))
        drifted = [dict(receipts[0]), {**receipts[1], "reasoningEffort": "low"}]
        self.assertTrue(validate_role_receipts(drifted))

    def test_campaign_validator_enforces_frozen_matrix_and_blind_confirmation(self) -> None:
        from terra_v3_contracts import validate_campaign_manifest

        campaign, cases, arms, roles = self._valid_campaign_contract()
        kwargs = {
            "suite_cases": cases, "arm_contracts": arms, "role_contract": roles,
            "campaign_ready": True, "public_oracle_receipt_digest": "d" * 64,
            "public_oracle_corpus_digest": "b" * 64,
        }
        self.assertEqual(validate_campaign_manifest(campaign, **kwargs), [])

        partial = json.loads(json.dumps(campaign))
        partial["runs"].pop()
        self.assertTrue(validate_campaign_manifest(partial, **kwargs))
        unknown = json.loads(json.dumps(campaign))
        unknown["runs"][0]["caseId"] = "invented"
        self.assertTrue(validate_campaign_manifest(unknown, **kwargs))
        unconfirmed = json.loads(json.dumps(campaign))
        unconfirmed["confirmationJudges"][0]["orderReversed"] = False
        self.assertTrue(validate_campaign_manifest(unconfirmed, **kwargs))
        self.assertTrue(validate_campaign_manifest(campaign, **{**kwargs, "campaign_ready": False}))
        wrong_oracle = json.loads(json.dumps(campaign))
        wrong_oracle["publicOracleReceiptDigest"] = "0" * 64
        self.assertTrue(validate_campaign_manifest(wrong_oracle, **kwargs))
        wrong_corpus = json.loads(json.dumps(campaign))
        wrong_corpus["runs"][0]["corpusLockDigest"] = "0" * 64
        self.assertTrue(validate_campaign_manifest(wrong_corpus, **kwargs))

    def test_campaign_validator_rejects_fabricated_receipts_tokens_metrics_and_trajectory(self) -> None:
        from terra_v3_contracts import validate_campaign_manifest

        campaign, cases, arms, roles = self._valid_campaign_contract()
        kwargs = {
            "suite_cases": cases, "arm_contracts": arms, "role_contract": roles,
            "campaign_ready": True, "public_oracle_receipt_digest": "d" * 64,
            "public_oracle_corpus_digest": "b" * 64,
        }
        mutations = []
        bad_tool = json.loads(json.dumps(campaign))
        bad_tool["runs"][0]["toolReceipt"]["arm"] = "ripgrep"
        mutations.append(bad_tool)
        bad_tokens = json.loads(json.dumps(campaign))
        bad_tokens["runs"][0]["tokenUsage"]["totalTokens"] = 999
        mutations.append(bad_tokens)
        estimated_tokens = json.loads(json.dumps(campaign))
        estimated_tokens["runs"][0]["tokenUsage"]["source"] = "estimated-from-characters"
        mutations.append(estimated_tokens)
        source_less_tokens = json.loads(json.dumps(campaign))
        del source_less_tokens["roleReceipts"][0]["tokenUsage"]["source"]
        mutations.append(source_less_tokens)
        bad_metrics = json.loads(json.dumps(campaign))
        del bad_metrics["runs"][0]["metrics"]["peakRssBytes"]
        mutations.append(bad_metrics)
        bad_trajectory = json.loads(json.dumps(campaign))
        bad_trajectory["runs"][0]["trajectory"]["records"][0]["attempt_index"] = 2
        mutations.append(bad_trajectory)
        missing_run_anchor = json.loads(json.dumps(campaign))
        del missing_run_anchor["runs"][0]["anchorResult"]
        mutations.append(missing_run_anchor)
        missing_receipt = json.loads(json.dumps(campaign))
        missing_receipt["roleReceipts"].pop()
        mutations.append(missing_receipt)
        receipt_token_drift = json.loads(json.dumps(campaign))
        receipt_token_drift["roleReceipts"][0]["tokenUsage"]["inputTokens"] = -1
        mutations.append(receipt_token_drift)
        for invalid in mutations:
            self.assertTrue(validate_campaign_manifest(invalid, **kwargs))

    def test_campaign_validator_rejects_malformed_quality_and_identity_revelation(self) -> None:
        from terra_v3_contracts import validate_campaign_manifest

        campaign, cases, arms, roles = self._valid_campaign_contract()
        kwargs = {
            "suite_cases": cases, "arm_contracts": arms, "role_contract": roles,
            "campaign_ready": True, "public_oracle_receipt_digest": "d" * 64,
            "public_oracle_corpus_digest": "b" * 64,
        }
        bad_score = json.loads(json.dumps(campaign))
        bad_score["judges"][0]["scores"]["X"]["correctness"] = 11
        self.assertTrue(validate_campaign_manifest(bad_score, **kwargs))
        missing_dimension = json.loads(json.dumps(campaign))
        del missing_dimension["judges"][0]["scores"]["Y"]["workflow"]
        self.assertTrue(validate_campaign_manifest(missing_dimension, **kwargs))
        missing_anchor = json.loads(json.dumps(campaign))
        del missing_anchor["judges"][0]["anchorResult"]
        self.assertTrue(validate_campaign_manifest(missing_anchor, **kwargs))
        revealed = json.loads(json.dumps(campaign))
        revealed["judges"][0]["labelToArm"] = {"X": "octocode", "Y": "ripgrep"}
        self.assertTrue(validate_campaign_manifest(revealed, **kwargs))
        bad_assignment = json.loads(json.dumps(campaign))
        bad_assignment["blindAssignments"][0]["answerDigests"]["X"] = "0" * 64
        self.assertTrue(validate_campaign_manifest(bad_assignment, **kwargs))
        not_reversed = json.loads(json.dumps(campaign))
        not_reversed["blindAssignments"][1]["labelToArm"] = {"X": "octocode", "Y": "ripgrep"}
        not_reversed["blindAssignments"][1]["answerDigests"] = dict(
            not_reversed["blindAssignments"][0]["answerDigests"]
        )
        unsigned = {key: value for key, value in not_reversed["blindAssignments"][1].items() if key != "assignmentDigest"}
        not_reversed["blindAssignments"][1]["assignmentDigest"] = hashlib.sha256(
            json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        self.assertTrue(validate_campaign_manifest(not_reversed, **kwargs))

    def test_sourcegraph_receipt_must_index_the_locked_commits(self) -> None:
        from terra_v3_contracts import validate_sourcegraph_receipt

        lock = {"repositories": [
            {"repo": "langchain-ai/langchain", "commit": "a" * 40},
            {"repo": "vercel/next.js", "commit": "b" * 40},
        ]}
        receipt = {"instanceDigest": "c" * 64, "repositories": [
            {"repo": "langchain-ai/langchain", "indexedCommit": "a" * 40, "indexConverged": True},
            {"repo": "vercel/next.js", "indexedCommit": "b" * 40, "indexConverged": True},
        ]}
        self.assertEqual(validate_sourcegraph_receipt(receipt, lock), [])
        receipt["repositories"][1]["indexedCommit"] = "d" * 40
        self.assertTrue(validate_sourcegraph_receipt(receipt, lock))


if __name__ == "__main__":
    unittest.main()
