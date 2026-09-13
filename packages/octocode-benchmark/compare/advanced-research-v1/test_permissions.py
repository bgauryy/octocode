"""Permissions are tested before any provider launch; no model calls."""
import json
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import test_pilot

pilot = test_pilot.pilot


class PermissionPreflight(unittest.TestCase):
    setUp = test_pilot.PilotControls.setUp
    tearDown = test_pilot.PilotControls.tearDown

    def test_remote_profile_keeps_filesystem_readonly_without_legacy_override(self):
        args = pilot.permission_args(True)
        self.assertNotIn("--sandbox", args)
        self.assertIn('default_permissions="octocode-benchmark-readonly-net"', args)
        self.assertIn('permissions.octocode-benchmark-readonly-net.extends=":read-only"', args)
        self.assertIn('permissions.octocode-benchmark-readonly-net.network.enabled=true', args)
        self.assertEqual(pilot.permission_args(False), ["--sandbox", "read-only"])

    def test_local_preflight_never_probes_network(self):
        with patch.object(pilot.subprocess, "run", side_effect=AssertionError("network probe in local campaign")):
            receipt = pilot.preflight_permissions(False, self.root)
        self.assertTrue(receipt["passed"])
        self.assertEqual(receipt["networkProbe"], "not_requested")

    def probe(self, **updates):
        result = {"network": {"status": 200, "repository": "vercel/next.js"},
                  "write": {"allowed": False, "errorCode": "EPERM"}}
        result.update(updates)
        return SimpleNamespace(returncode=0, stdout=json.dumps(result), stderr="")

    def test_probe_and_trial_share_exact_profile_arguments(self):
        with patch.object(pilot.subprocess, "run", return_value=self.probe()) as run:
            receipt = pilot.preflight_permissions(True, self.root)
        self.assertTrue(receipt["passed"])
        probe_argv = run.call_args.args[0]
        self.assertEqual(probe_argv[:4], ["codex", "sandbox", "-P", "octocode-benchmark-readonly-net"])
        permissions = pilot.permission_args(True)
        self.assertEqual(probe_argv[4:4 + len(permissions)], permissions)
        args = SimpleNamespace(output_dir=self.root, cli=self.cli, remote=True, tool_context="Canonical")
        with patch.object(pilot, "monitor", return_value={"answer": "Answer", "eligible": False, "failures": []}) as monitor, patch("builtins.print"):
            pilot.trial("A15", "octocode", 1, "Question", args,
                        {"langchain": self.corpus, "nextjs": self.corpus}, pilot.Budgets())
        trial_argv = monitor.call_args.args[0]
        self.assertNotIn("--sandbox", trial_argv)
        start = trial_argv.index("-c")
        self.assertEqual(trial_argv[start:start + len(permissions)], permissions)
        self.assertIn("--ignore-user-config", trial_argv)

    def test_missing_network_or_write_protection_fails_closed(self):
        for result in (self.probe(network={"error": "ENOTFOUND"}),
                       self.probe(network={"status": 403, "repository": "vercel/next.js"}),
                       self.probe(write={"allowed": True}),
                       self.probe(write={"allowed": False, "errorCode": "ENOENT"}),
                       SimpleNamespace(returncode=1, stdout="", stderr="sandbox failure")):
            with self.subTest(result=result), patch.object(pilot.subprocess, "run", return_value=result):
                self.assertFalse(pilot.preflight_permissions(True, self.root)["passed"])

    def test_probe_timeout_has_no_retry_or_unsandboxed_fallback(self):
        with patch.object(pilot.subprocess, "run", side_effect=pilot.subprocess.TimeoutExpired("codex sandbox", 20)) as run:
            receipt = pilot.preflight_permissions(True, self.root)
        self.assertFalse(receipt["passed"])
        self.assertIn("TimeoutExpired", receipt["error"])
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args.args[0][:2], ["codex", "sandbox"])

    def test_failed_preflight_blocks_all_model_launches_and_seals_failure(self):
        output = self.root / "failed-campaign"
        with patch.object(pilot, "preflight_permissions", return_value={"passed": False, "networkProbe": "failed"}), \
                patch.object(pilot, "trial", side_effect=AssertionError("model launched")), \
                patch.object(pilot.subprocess, "Popen", side_effect=AssertionError("model launched")):
            status = pilot.main(["--run", "--remote", "--cases", "A01", "--langchain", str(self.corpus),
                                 "--nextjs", str(self.corpus), "--output-dir", str(output)])
        self.assertEqual(status, 1)
        report = json.loads((output / "report.json").read_text())
        self.assertEqual(report["results"], [])
        self.assertIn("permission_preflight_failed", report["fatalError"])
        self.assertFalse(json.loads((output / "permission-preflight.json").read_text())["passed"])
        self.assertEqual((output / "permission-preflight.json").stat().st_mode & 0o222, 0)


if __name__ == "__main__":
    unittest.main()
