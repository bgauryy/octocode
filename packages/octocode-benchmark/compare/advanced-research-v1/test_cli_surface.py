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

    def test_native_field_inputs_use_canonical_parser_and_all_scope_checks(self):
        bridge = pilot.cli_input.prepare_bridge(self.root / "parser")
        policy = pilot.Policy("octocode", self.cli, [self.corpus], flag_bridge=bridge)
        def call(tool, *tail):
            return policy.audit(shlex.join(["node", str(self.cli), "tools", tool, *tail]))
        self.assertIsNone(call("localFetch", "--path", str(self.source), "--start-line", "1", "--end-line=1"))
        self.assertEqual(call("localFetch", "--path", "/outside/source.py"), "query_outside_corpus")
        self.assertIsNone(call("astSearch", "topology", "reachability", "--path", str(self.corpus),
                               "--entrypoints", str(self.source)))
        self.assertEqual(call("astSearch", "topology", "reachability", "--path", str(self.corpus),
                               "--entrypoints", str(self.source), "--entrypoints", "/outside/source.py"),
                         "query_outside_corpus")
        self.assertEqual(call("localFetch", "--path", str(self.source), "--unknown"),
                         "recoverable_cli_syntax:invalid_field_flags")
        self.assertIsNone(call("localFetch", "--help"))

    def test_read_only_word_counts_keep_every_path_scoped(self):
        for flags in ([], ["-c"], ["-m"], ["-w"], ["-lc"], ["-l", "-c"]):
            self.assertIsNone(self.policy.audit(shlex.join(["wc", *flags, str(self.source)])))
        self.assertIsNone(self.policy.audit(shlex.join(["wc", "-c", str(self.source), str(self.source)])))
        self.assertIsNotNone(self.policy.audit(shlex.join(["wc", "-c", str(self.source), "/outside/source.py"])))
        self.assertIsNotNone(self.policy.audit("wc -c"))
        self.assertIsNotNone(self.policy.audit(shlex.join(["wc", "--files0-from", str(self.source)])))
        actual = subprocess.run(["wc", "-c", str(self.source)], capture_output=True, text=True, check=True)
        self.assertEqual(int(actual.stdout.split()[0]), len(self.source.read_bytes()))

    def test_multiple_tool_names_get_actual_cli_error_without_tool_execution(self):
        cli = pilot.WORKSPACE / "packages/octocode/out/octocode.js"
        policy = pilot.Policy("octocode", cli, [self.corpus], remote=True)
        args = ["node", str(cli), "tools", "localFetch", "ghGetFileContent", "--queries",
                '[{"path":"/outside/source.py"},{"owner":"outside","repo":"scope"}]']
        self.assertEqual(policy.audit(shlex.join(args)), "recoverable_cli_syntax:multiple_tool_names")
        actual = subprocess.run(args, capture_output=True, text=True, timeout=20)
        self.assertNotEqual(actual.returncode, 0)
        self.assertIn("positional selector", actual.stdout + actual.stderr)
        self.assertNotIn("source.py", actual.stdout + actual.stderr)
        self.assertEqual(policy.audit(shlex.join(args + ["--unexpected"])), "unsupported_cli_flags")


if __name__ == "__main__":
    unittest.main()
