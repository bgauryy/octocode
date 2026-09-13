"""Equivalent executable forms retain tool, query, and corpus controls."""
import shlex
import shutil
import json
import subprocess
import tempfile
from pathlib import Path
import unittest

import test_pilot

pilot = test_pilot.pilot


class CliSurface(unittest.TestCase):
    setUp = test_pilot.PilotControls.setUp
    tearDown = test_pilot.PilotControls.tearDown

    def test_equivalent_cli_launches_keep_scope_checks(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        launches = ([str(self.cli)], ["node", str(self.cli)],
                    [shutil.which("node"), str(self.cli)])
        for launch in launches:
            with self.subTest(launch=launch):
                self.assertIsNone(policy.audit(shlex.join(launch + ["tools", "localFetch", "--scheme"])))
                self.assertEqual(policy.audit(shlex.join(launch + ["tools", "localFetch", "--queries",
                    '{"path":"/outside/source.py"}'])), "query_outside_corpus")
                self.assertEqual(policy.audit(shlex.join(launch + ["localFetch", "--queries",
                    '{"path":"' + str(self.source) + '"}'])), "recoverable_cli_syntax:missing_tools_subcommand")
        self.assertEqual(policy.audit(shlex.join([str(self.root / "other.js"), "tools"])),
                         "wrong_octocode_executable")

    def test_targeted_reads_use_the_shared_output_budget(self):
        policy = pilot.Policy("raw-tools", self.cli, [self.corpus])
        for tool in ("head", "tail"):
            self.assertIsNone(policy.audit(shlex.join([tool, "-n", "200", str(self.source)])))
            self.assertIsNotNone(policy.audit(shlex.join([tool, "-n", "200", "/outside/source.py"])))

    def test_supplied_evidence_mode_does_not_require_a_redundant_tool_call(self):
        audit = pilot.EventAudit(self.policy, pilot.Budgets(), require_commands=False)
        audit.feed({"type": "item.completed", "item": {
            "id": "answer", "type": "agent_message", "text": "Evidence source.py:1"}})
        audit.feed(test_pilot.usage())
        self.assertTrue(audit.finish(0)["eligible"])
        ordinary = pilot.EventAudit(self.policy, pilot.Budgets())
        self.assertIn("no_research_commands", ordinary.finish(0)["failures"])

    def test_invalid_json_can_be_repaired_after_real_cli_rejection(self):
        fixture = tempfile.TemporaryDirectory(dir=pilot.WORKSPACE / ".octocode/tmp", prefix="repair fixture ")
        self.addCleanup(fixture.cleanup)
        self.corpus = Path(fixture.name)
        self.source = self.corpus / "source.py"
        self.source.write_text("pass\n")
        cli = pilot.WORKSPACE / "packages/octocode/out/octocode.js"
        policy = pilot.Policy("octocode", cli, [self.corpus])
        malformed = '{"path":"/outside/source.py",'
        args = ["node", str(cli), "tools", "localFetch", "--queries", malformed]
        command = shlex.join(args)
        self.assertEqual(policy.audit(command), "recoverable_cli_syntax:invalid_query_json")
        failed = subprocess.run(args, capture_output=True, text=True, timeout=20)
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("JSON", failed.stdout + failed.stderr)
        audit = pilot.EventAudit(policy, pilot.Budgets(max_calls=2))
        audit.feed({"type": "item.completed", "item": {
            "id": "bad", "type": "command_execution", "command": command,
            "aggregated_output": failed.stdout + failed.stderr, "exit_code": failed.returncode}})
        self.assertFalse(audit.stop_requested)
        args[-1] = json.dumps({"path": str(self.source)})
        repaired = subprocess.run(args, capture_output=True, text=True, timeout=20)
        self.assertEqual(repaired.returncode, 0, repaired.stderr)
        self.assertIn("pass", repaired.stdout)
        audit.feed({"type": "item.completed", "item": {
            "id": "fixed", "type": "command_execution", "command": shlex.join(args),
            "aggregated_output": repaired.stdout, "exit_code": 0}})
        audit.feed({"type": "item.completed", "item": {
            "id": "answer", "type": "agent_message", "text": "source.py:1 contains pass."}})
        audit.feed(test_pilot.usage())
        result = audit.finish(0)
        self.assertTrue(result["eligible"], result["failures"])
        self.assertEqual(result["shellCalls"], 2)
        self.assertEqual(result["commands"][1]["followsRecoverableCommandError"], "bad")
        args[-1] = '{"path":"/outside/source.py"}'
        self.assertEqual(policy.audit(shlex.join(args)), "query_outside_corpus")
        args[-1] = malformed
        self.assertEqual(policy.audit(shlex.join(args + ["--unexpected"])), "unsupported_cli_flags")

    def test_quoted_newlines_are_argument_data_and_keep_scope_checks(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        query = json.dumps({"path": str(self.source)}, indent=2)
        command = shlex.join(["node", str(self.cli), "tools", "localFetch", "--queries", query])
        self.assertIsNone(policy.audit(command))
        self.assertIsNone(self.policy.audit(shlex.join(["rg", "-U", "-F", "pass\n", str(self.source)])))
        self.assertIsNotNone(policy.audit(command + "\nid"))
        self.assertIsNotNone(policy.audit(command + ' "$(id)"'))


if __name__ == "__main__":
    unittest.main()
