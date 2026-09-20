"""Fresh-protocol regressions; no provider calls."""
import json
import shlex
import unittest
from unittest.mock import patch

import test_pilot

pilot = test_pilot.pilot


class CampaignContext(unittest.TestCase):
    setUp = test_pilot.PilotControls.setUp
    tearDown = test_pilot.PilotControls.tearDown

    def test_canonical_discovery_grammar_and_budget(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        for suffix in ("scheme", "scheme --compact", "scheme --help",
                       "scheme localFetch", "scheme localFetch --view query --compact"):
            self.assertIsNone(policy.audit(f"node {self.cli} {suffix}"), suffix)
        for suffix in ("scheme --path /tmp", "scheme --compact --compact",
                       "scheme localFetch --view invalid", "scheme astRewrite",
                       "scheme ghCloneRepo", "tools", "context --full", "skill install x"):
            self.assertIsNotNone(policy.audit(f"node {self.cli} {suffix}"), suffix)
        audit = pilot.EventAudit(policy, pilot.Budgets(max_calls=1))
        for ident, suffix in (("c1", "scheme --compact"),
                              ("c2", "scheme localFetch --view query --compact")):
            audit.feed({"type": "item.started", "item": {"id": ident, "type": "command_execution",
                        "command": f"node {self.cli} {suffix}"}})
        self.assertIn("call_budget_exceeded", audit.failures)
        self.assertFalse(any(reason.startswith("surface_violation") for reason in audit.failures))

    def test_canonical_discovery_executes_real_cli(self):
        import subprocess
        cli = pilot.WORKSPACE / "packages/octocode/out/octocode.js"
        policy = pilot.Policy("octocode", cli, [self.corpus])
        for args in (("scheme", "--compact"),
                     ("scheme", "localFetch", "--view", "query", "--compact")):
            argv = ["node", str(cli), *args]
            with self.subTest(args=args):
                self.assertIsNone(policy.audit(shlex.join(argv)))
                result = subprocess.run(argv, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                if args == ("scheme", "--compact"):
                    self.assertIn("localSearch", {row["name"] for row in json.loads(result.stdout)["tools"]})
                else:
                    self.assertEqual(json.loads(result.stdout)["name"], "localFetch")
    def test_cli_query_envelopes_preserve_every_scope_check(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        query = {"reasoning": "Read the scoped fixture.", "path": str(self.source)}
        for payload in ({"queries": [query]}, [query], query):
            command = f"node {self.cli} localFetch {shlex.quote(json.dumps(payload))} --compact"
            self.assertIsNone(policy.audit(command))
        for payload in ({"queries": []}, {"queries": [query, {"reasoning": "Attempt an unscoped read.", "path": "/etc/passwd"}]},
                        {"queries": [query], "extra": True}):
            command = f"node {self.cli} localFetch {shlex.quote(json.dumps(payload))} --compact"
            self.assertIsNotNone(policy.audit(command))

    def test_response_continuation_envelope_preserves_scope_and_owns_no_schema_validation(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        payload = {"queries": [{"reasoning": "Read the scoped fixture.", "path": str(self.source)}], "responseCharLength": 1000,
                   "responseCharOffset": 1000, "responseSnapshot": "response-v1:fixture"}
        def audit(value):
            return policy.audit(f"node {self.cli} localFetch {shlex.quote(json.dumps(value))} --compact")
        self.assertIsNone(audit(payload))
        self.assertIsNone(audit({**payload, "responseCharLength": "invalid-runtime-schema-value"}))
        self.assertIsNotNone(audit({**payload, "extra": True}))
        self.assertIsNotNone(audit({**payload, "queries": [payload["queries"][0], {"reasoning": "Attempt an unscoped read.", "path": "/etc/passwd"}]}))

    def test_whole_response_continuations_execute_unmodified_through_real_cli(self):
        import subprocess
        import tempfile
        from pathlib import Path
        cli = pilot.WORKSPACE / "packages/octocode/out/octocode.js"
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parent, prefix="response fixture ") as root:
            source = Path(root) / "source.py"
            source.write_text("".join(f"fixture source line {line}\n" for line in range(1, 101)))
            query = {"queries": [{"reasoning": "Page the bounded local fixture.", "path": str(source), "startLine": 1, "endLine": 100}], "responseCharLength": 1024}
            policy = pilot.Policy("octocode", cli, [root])
            offsets, pages = [], []
            while True:
                argv = ["node", str(cli), "localFetch", json.dumps(query), "--compact"]
                self.assertIsNone(policy.audit(shlex.join(argv)))
                result = subprocess.run(argv, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                page = json.loads(result.stdout)
                pagination = page["responsePagination"]
                self.assertFalse(pagination.get("restart", False))
                offsets.append(pagination["charOffset"])
                pages.append(pagination)
                if not pagination["hasMore"]:
                    break
                self.assertLess(len(pages), 20, "continuation did not finish bounded fixture")
                continuation = pagination["next"]
                self.assertEqual(continuation["tool"], "localFetch")
                query = continuation["query"]
            self.assertGreater(len(pages), 1)
            self.assertEqual(offsets, sorted(set(offsets)))
            self.assertEqual(offsets[0], 0)
            self.assertEqual(sum(page["charLength"] for page in pages), pages[-1]["totalChars"])
            for before, after in zip(pages, pages[1:]):
                self.assertEqual(before["charOffset"] + before["charLength"], after["charOffset"])

    def test_supported_envelope_executes_real_cli_and_returns_source(self):
        import subprocess
        import tempfile
        from pathlib import Path
        cli = pilot.WORKSPACE / "packages/octocode/out/octocode.js"
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parent, prefix="envelope fixture ") as root:
            source = Path(root) / "source.py"
            source.write_text("pass\n")
            payload = {"queries": [{"reasoning": "Read the bounded local fixture.", "path": str(source), "startLine": 1, "endLine": 1}]}
            argv = ["node", str(cli), "localFetch", json.dumps(payload), "--compact"]
            self.assertIsNone(pilot.Policy("octocode", cli, [root]).audit(shlex.join(argv)))
            result = subprocess.run(argv, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            data = json.loads(result.stdout)["results"][0]["data"]
            self.assertEqual(data["content"], "pass\n")
            self.assertEqual(data["sourceLineRanges"], [{"start": 1, "end": 1}])

    def test_context_is_captured_verbatim_and_not_replaced_by_harness_guidance(self):
        catalog = {"tools": [{"name": name, "availability": {"enabled": True}}
                             for name in pilot.Policy.local_tools],
                   "instructions": "Canonical context, unchanged.\n"}
        context = "Canonical context, unchanged.\n"
        def contract_command(argv, cwd=pilot.WORKSPACE):
            if argv[-2:] == ["scheme", "--compact"]:
                return json.dumps(catalog)
            return json.dumps({"name": argv[3]})
        with patch.object(pilot, "command", side_effect=contract_command):
            contract = pilot.capture_tool_contract(self.cli)
        self.assertEqual(contract["context"], context)
        prompt = pilot.prompt_for("A01", "octocode", "Question", self.cli,
                                  {"langchain": self.corpus, "nextjs": self.corpus},
                                  pilot.Budgets(), context=context)
        self.assertIn(context, prompt)
        self.assertNotIn("CHOSEN_SUBTREE", prompt)
        self.assertNotIn("localSearch --queries", prompt)

    def test_read_only_remote_scope_requires_selected_repository_and_pinned_content(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus], remote=True)
        query = {"reasoning": "Read the pinned remote fixture.", "owner": "vercel", "repo": "next.js",
                 "path": "packages/next/src/server/config.ts", "branch": pilot.COMMITS["nextjs"]}
        def audit(tool, value):
            return policy.audit(f"node {self.cli} {tool} {shlex.quote(json.dumps(value))} --compact")
        self.assertIsNone(audit("ghGetFileContent", query))
        self.assertIsNotNone(audit("ghGetFileContent", {**query, "branch": "canary"}))
        self.assertIsNotNone(audit("ghGetFileContent", {**query, "owner": "other"}))
        self.assertIsNotNone(audit("ghCloneRepo", query))
        self.assertIsNotNone(audit("astRewrite", {"path": str(self.source), "operation": "apply"}))

    def test_raw_github_reads_reject_mutation_and_other_repositories(self):
        policy = pilot.Policy("raw-tools", self.cli, [self.corpus], remote=True)
        endpoint = f"repos/vercel/next.js/contents/packages/next/src/server/config.ts?ref={pilot.COMMITS['nextjs']}"
        self.assertIsNone(policy.audit(f"gh api {shlex.quote(endpoint)} --method GET"))
        self.assertIsNotNone(policy.audit("gh api repos/other/repo/issues --method GET"))
        self.assertIsNotNone(policy.audit("gh api repos/vercel/next.js/issues --method POST"))
        self.assertIsNotNone(policy.audit("gh api repos/vercel/next.js/contents/file?ref=canary --method GET"))

    def test_single_pass_counterbalances_arm_order_across_cases(self):
        helper = test_pilot.PilotControls()
        helper.root, helper.corpus, helper.cli = self.root, self.corpus, self.cli
        _, launches, _, _ = helper.mock_campaign()
        first = [(case, arm) for case, arm, number in launches if number == 1]
        self.assertEqual(first, [("A01", "octocode"), ("A01", "raw-tools"),
                                 ("A02", "raw-tools"), ("A02", "octocode")])


if __name__ == "__main__":
    unittest.main()
