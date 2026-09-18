"""Deterministic pilot controls; never launch a model."""
import importlib.util
import json
from pathlib import Path
import shlex
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("advanced_pilot", Path(__file__).with_name("pilot.py"))
pilot = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = pilot
SPEC.loader.exec_module(pilot)


def usage(**changes):
    value = dict(input_tokens=100, cached_input_tokens=40, output_tokens=20,
                 reasoning_output_tokens=8, cache_write_input_tokens=0)
    value.update(changes)
    return {"type": "turn.completed", "usage": value}


class PilotControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.corpus = self.root / "corpus"
        self.corpus.mkdir()
        self.source = self.corpus / "source.py"
        self.source.write_text("pass\n")
        self.cli = self.root / "octocode.js"
        self.policy = pilot.Policy("raw-tools", self.cli, [self.corpus])

    def tearDown(self):
        self.temp.cleanup()

    def state(self, **kw):
        return pilot.EventAudit(self.policy, pilot.Budgets(**kw))

    def command(self, ident="c1", command=None, output="ok", exit_code=0, kind="item.completed"):
        return {"type": kind, "item": {"id": ident, "type": "command_execution",
                "command": command or f"rg -n -F pass {self.source}",
                "aggregated_output": output, "exit_code": exit_code}}

    def finish(self, state):
        state.feed({"type": "item.completed", "item": {
            "id": "answer", "type": "agent_message", "text": "Answer source.py:1"}})
        state.feed(usage())
        return state.finish(0)

    def test_native_subsets_are_not_added_to_total(self):
        state = self.state()
        state.feed(self.command())
        result = self.finish(state)
        self.assertTrue(result["eligible"])
        self.assertEqual(result["usage"]["totalTokens"], 120)
        self.assertEqual(result["usage"]["uncachedInputTokens"], 60)
        self.assertIsNone(result["modelVisibleToolOutputBytes"])

    def test_missing_native_subsets_are_unknown_without_losing_total_usage(self):
        state = self.state()
        state.feed(self.command())
        event = usage()
        for key in ("reasoning_output_tokens", "cached_input_tokens", "cache_write_input_tokens"):
            del event["usage"][key]
        state.feed(event)
        state.feed({"type": "item.completed", "item": {
            "id": "answer", "type": "agent_message", "text": "Answer source.py:1"}})
        result = state.finish(0)
        self.assertIsNone(result["usage"]["reasoningOutputTokens"])
        self.assertIsNone(result["usage"]["uncachedInputTokens"])
        self.assertEqual(result["usage"]["totalTokens"], 120)
        self.assertTrue(result["eligible"], result["failures"])

    def test_required_usage_and_invalid_present_subsets_still_fail(self):
        for key in ("input_tokens", "output_tokens"):
            state = self.state()
            event = usage()
            del event["usage"][key]
            state.feed(event)
            result = state.finish(0)
            self.assertIsNone(result["usage"]["totalTokens"])
            self.assertIn("missing_usage:" + key, result["failures"])
        for value in (-1, True, "10"):
            state = self.state()
            state.feed(usage(reasoning_output_tokens=value))
            self.assertIn("missing_usage:reasoning_output_tokens", state.finish(0)["failures"])

    def test_cumulative_reset_is_not_summed(self):
        state = self.state()
        state.feed(usage())
        state.feed(usage(input_tokens=10, cached_input_tokens=0))
        result = state.finish(0)
        self.assertIn("usage_counter_reset", result["failures"])
        self.assertIsNone(result["usage"]["totalTokens"])

    def test_output_counter_reset_is_explicit(self):
        state = self.state()
        state.feed(self.command(kind="item.updated", output="first"))
        state.feed(self.command(output="next"))
        self.assertIn("raw_output_counter_reset:c1", state.finish(0)["failures"])

    def test_malformed_native_receipts_do_not_crash(self):
        for receipts in ([None], [None, None]):
            state = self.state()
            state.receipts = receipts
            self.assertIn("malformed_usage_receipt", state.finish(0)["failures"])

    def test_bad_subsets_fail(self):
        state = self.state()
        state.feed(usage(cached_input_tokens=101, reasoning_output_tokens=21))
        self.assertIn("invalid_usage_subsets", state.finish(0)["failures"])

    def test_started_completed_updates_count_once(self):
        state = self.state(max_calls=1)
        state.feed(self.command(kind="item.started", output=""))
        state.feed(self.command(kind="item.updated", output="ok"))
        state.feed(self.command())
        result = self.finish(state)
        self.assertEqual(result["shellCalls"], 1)
        self.assertEqual(result["rawToolOutputBytes"], 2)
        self.assertTrue(result["eligible"])

    def test_call_budget_fails_at_start_not_just_completion(self):
        state = self.state(max_calls=1)
        state.feed(self.command())
        state.feed(self.command("c2", kind="item.started"))
        self.assertTrue(state.stop_requested)
        self.assertIn("call_budget_exceeded", state.failures)

    def test_output_budget_and_error_are_preserved(self):
        state = self.state(max_output_bytes=3)
        state.feed(self.command(output="large", exit_code=2))
        result = self.finish(state)
        self.assertFalse(result["eligible"])
        self.assertEqual(result["commands"][0]["exitCode"], 2)
        self.assertIn("call_output_budget_exceeded:c1", result["failures"])

    def test_runtime_errors_and_missing_usage(self):
        state = self.state()
        state.feed({"type": "error", "message": "provider unavailable"})
        result = state.finish(-15, timed_out=True)
        for expected in ("process_timeout", "provider_error", "missing_usage_receipt"):
            self.assertIn(expected, result["failures"])

    def test_non_shell_native_tools_fail_closed(self):
        state = self.state()
        state.feed({"type": "item.started", "item": {"id": "x", "type": "mcp_tool_call"}})
        self.assertTrue(state.stop_requested)

    def test_shell_wrapper_and_quote_variants(self):
        command = f"rg -n -F 'a b' {shlex.quote(str(self.source))}"
        for prefix in ("/bin/zsh -lc ", "bash -c ", "/bin/sh -c "):
            self.assertIsNone(self.policy.audit(prefix + shlex.quote(command)))
        self.assertIsNone(self.policy.audit(f"sed -n '1,20p' {self.source}"))

    def test_quoted_punctuation_is_pattern_data_not_shell_composition(self):
        for pattern in ("|", "&&", ";", "<", ">", "(", ")"):
            command = f"rg -n -F {shlex.quote(pattern)} {self.source}"
            with self.subTest(pattern=pattern):
                self.assertIsNone(self.policy.audit(command))
                self.assertIsNone(self.policy.audit("/bin/zsh -lc " + shlex.quote(command)))
        self.assertIsNone(self.policy.audit(f"rg -F \\| {self.source}"))
        self.assertEqual(self.policy.audit(f"rg -F '|' {self.source} | head -n 10"), "shell_composition")

    def test_lsp_queries_use_their_uri_or_workspace_root_contract(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        for query in ({"operation": "documentSymbols", "uri": str(self.source)},
                      {"operation": "documentSymbols", "uri": self.source.as_uri()},
                      {"operation": "workspaceSymbol", "workspaceRoot": str(self.corpus), "symbolName": "source"}):
            call = f"node {self.cli} tools lspSearch --queries {shlex.quote(json.dumps(query))} --compact"
            self.assertIsNone(policy.audit(call), query)

    def test_secondary_query_paths_cannot_escape_corpus(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        outside = self.root / "outside.py"
        outside.write_text("pass\n")
        (self.corpus / "escape.py").symlink_to(outside)
        queries = [
            ("lspSearch", {"path": str(self.corpus), "uri": outside.as_uri()}),
            ("lspSearch", {"uri": str(self.source), "workspaceRoot": str(self.root)}),
            ("lspSearch", {"uri": (self.corpus / "escape.py").as_uri()}),
            ("lspSearch", {"uri": "file://remote-host" + str(self.source)}),
            ("lspSearch", {"uri": self.corpus.as_uri() + "/%2e%2e/outside.py"}),
            ("astSearch", {"operation": "topology", "analysis": "path", "path": str(self.corpus), "file": str(self.source), "target": str(outside)}),
            ("astSearch", {"operation": "topology", "analysis": "dependencies", "path": str(self.corpus), "file": "../outside.py"}),
            ("astSearch", {"operation": "topology", "analysis": "reachability", "path": str(self.corpus), "entrypoints": [str(outside)]}),
        ]
        for tool, query in queries:
            with self.subTest(query=query):
                call = f"node {self.cli} tools {tool} --queries {shlex.quote(json.dumps(query))} --compact"
                self.assertEqual(policy.audit(call), "query_outside_corpus")

    def test_scoped_topology_supports_root_inference_and_relative_targets(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        for query in ({"operation": "topology", "analysis": "dependencies", "file": str(self.source)},
                      {"operation": "topology", "analysis": "path", "path": str(self.corpus), "file": "source.py", "target": "source.py"}):
            call = f"node {self.cli} tools astSearch --queries {shlex.quote(json.dumps(query))} --compact"
            self.assertIsNone(policy.audit(call), query)

    def test_exec_network_expansion_and_sed_program_rejected(self):
        for command in ("curl https://example.com", "python -c 'print(1)'",
                        f"rg --pre cat pass {self.source}",
                        f"sed -n '1e id' {self.source}",
                        f"rg pass {self.source} | head", "rg $(pwd)",
                        f"rg pass {self.source} > /tmp/result",
                        f"env rg pass {self.source}"):
            self.assertIsNotNone(self.policy.audit(command), command)

    def test_structural_metavariables_must_be_shell_literals(self):
        pattern = "const $NAME = createContext<$T>($$$ARGS)"
        self.assertIsNone(self.policy.audit(
            f"ast-grep run -p {shlex.quote(pattern)} --lang ts --json=compact {self.corpus}"))
        self.assertIsNotNone(self.policy.audit(
            f'ast-grep run -p "{pattern}" --lang ts {self.corpus}'))

    def test_total_output_budget(self):
        state = self.state(max_output_bytes=5, max_total_output_bytes=6)
        state.feed(self.command(output="four"))
        state.feed(self.command("c2", output="four"))
        self.assertIn("total_output_budget_exceeded", state.failures)

    def test_no_run_flag_never_launches_subprocess(self):
        from unittest.mock import patch
        with patch.object(pilot.subprocess, "Popen", side_effect=AssertionError("model launched")):
            with patch("builtins.print"):
                self.assertEqual(pilot.main(["--output-dir", str(self.root / "unused"),
                                            "--langchain", str(self.corpus), "--nextjs", str(self.corpus)]), 0)
        self.assertFalse((self.root / "unused").exists())

    def test_fake_stream_stops_on_observed_call_budget(self):
        directory = self.root / "bounded"
        directory.mkdir()
        stream = json.dumps(self.command("c2", kind="item.started"))
        state = self.state(max_calls=1)
        state.feed(self.command())
        child = [sys.executable, "-u", "-c",
                 "import time; print(" + repr(stream) + ", flush=True); time.sleep(10)"]
        result = pilot.monitor(child, "prompt", directory, state)
        self.assertIn("call_budget_exceeded", result["failures"])
        self.assertLess(result["wallSeconds"], 3)

    def test_oversized_and_malformed_event_bytes_are_retained(self):
        for content, failure in (("x" * 2000, "event_size_budget_exceeded"),
                                 ("{broken}", "malformed_event")):
            with self.subTest(failure=failure):
                directory = self.root / failure
                directory.mkdir()
                state = self.state(max_event_bytes=1000)
                child = [sys.executable, "-u", "-c", "print(" + repr(content) + ")"]
                result = pilot.monitor(child, "prompt", directory, state)
                self.assertIn(failure, result["failures"])
                self.assertEqual((directory / "events.jsonl").read_text(), content + "\n")

    def test_outside_scope_and_symlink_escape_rejected(self):
        self.assertIsNotNone(self.policy.audit("rg pass /etc/passwd"))
        (self.corpus / "escape").symlink_to("/etc/passwd")
        self.assertIsNotNone(self.policy.audit(f"rg pass {self.corpus / 'escape'}"))

    def test_octocode_only_local_scoped_queries(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        query = json.dumps({"path": str(self.source), "startLine": 1, "endLine": 20})
        base = f"node {self.cli} tools localFetch --queries {shlex.quote(query)} --compact"
        self.assertIsNone(policy.audit(base))
        self.assertIsNotNone(policy.audit(base.replace(str(self.source), "/etc/passwd")))
        self.assertIsNotNone(policy.audit(f"node {self.cli} tools ghSearch --queries '{{}}'"))

    def test_smart_case_is_an_allowed_raw_search_flag(self):
        for flag in ("-S", "--smart-case", "-s", "--case-sensitive"):
            self.assertIsNone(self.policy.audit(
                f"rg -n {flag} --glob '*.py' 'pass' {self.corpus}"))

    def test_common_read_only_ripgrep_variants(self):
        for flags in ("-U", "--multiline --multiline-dotall", "-nUS", "-P -w",
                      "--pcre2 --word-regexp", "-A5 -B2 -m10", "--max-depth 3",
                      "--iglob '*.py' --stats", "--sort path --color never"):
            self.assertIsNone(self.policy.audit(f"rg {flags} 'pass' {self.corpus}"), flags)
        self.assertIsNone(self.policy.audit(f"rg -n -F -- '-literal' {self.source}"))
        for flags in ("--follow", "--pre=cat", "-f/etc/passwd", "--hostname-bin=id"):
            self.assertIsNotNone(self.policy.audit(f"rg {flags} pass {self.corpus}"))

    def test_missing_tools_is_recorded_and_recoverable_within_call_budget(self):
        state = pilot.EventAudit(pilot.Policy("octocode", self.cli, [self.corpus]), pilot.Budgets())
        query = shlex.quote(json.dumps({"path": str(self.source), "searchText": "pass"}))
        wrong = f"node {self.cli} localSearch --queries {query} --compact"
        correct = f"node {self.cli} tools localSearch --queries {query} --compact"
        state.feed(self.command(command=wrong, kind="item.started", output=""))
        self.assertFalse(state.stop_requested)
        state.feed(self.command(command=wrong, output="unknown command localSearch", exit_code=1))
        state.feed(self.command("c2", command=correct))
        result = self.finish(state)
        self.assertTrue(result["eligible"])
        self.assertEqual(result["shellCalls"], 2)
        self.assertEqual(result["recoverableCommandErrors"][0]["id"], "c1")
        self.assertEqual(result["nonzeroCommandExits"], 1)
        self.assertEqual(result["commands"][1]["followsRecoverableCommandError"], "c1")

    def test_recoverable_syntax_does_not_waive_scope_or_tool_restrictions(self):
        policy = pilot.Policy("octocode", self.cli, [self.corpus])
        for suffix in ("auth", "skill install foo", "ghSearch --queries '{}'",
                       "localFetch --queries '{\"path\":\"/etc/passwd\"}'"):
            issue = policy.audit(f"node {self.cli} {suffix}")
            self.assertIsNotNone(issue)
            self.assertFalse(issue.startswith("recoverable_cli_syntax:"))

    def test_immutable_receipts(self):
        dest = self.root / "receipt.json"
        pilot.write_once(dest, {"a": 1})
        with self.assertRaises(FileExistsError):
            pilot.write_once(dest, {"a": 2})

    def mock_campaign(self, drift=None, check_only=False):
        from unittest.mock import patch
        controls = self.root / ("controls-" + str(drift))
        controls.mkdir()
        (controls / "QUESTIONS.md").write_text("# Cases\n\n## A01 First\nQuestion one.\n\n## A02 Second\nQuestion two.\n")
        (controls / "RUBRIC.md").write_text("Frozen rubric.\n")
        runner = controls / "pilot.py"
        runner.write_text("Frozen runner.\n")
        permissions = controls / "sandbox_permissions.py"
        permissions.write_text("Frozen permission module.\n")
        parser_source = controls / "cli_input.py"
        parser_source.write_text("Frozen parser adapter.\n")
        bridge_source = controls / "cli_input_bridge.ts"
        bridge_source.write_text("Frozen parser bridge source.\n")
        output = self.root / ("campaign-" + str(drift))
        state = {"candidate": "before", "corpora": "before", "toolContract": "before"}
        launches = []

        def fake_bridge(directory):
            bridge = directory / "cli-input-bridge.cjs"
            bridge.write_text("Frozen canonical parser bundle.\n")
            return bridge

        def fake_trial(case, arm, pass_number, question, args, corpora, budgets):
            launches.append((case, arm, pass_number))
            directory = output / f"{case}-p{pass_number:02}-{arm}"
            directory.mkdir()
            result = {"case": case, "arm": arm, "passNumber": pass_number,
                      "eligible": True, "failures": []}
            pilot.write_once(directory / "answer.md", "Completed answer.\n")
            pilot.write_once(directory / "receipt.json", result)
            if len(launches) == 1 and drift:
                if drift in state:
                    state[drift] = "after"
                elif drift == "missing_rubric":
                    (controls / "RUBRIC.md").unlink()
                else:
                    target = {"runner": runner, "questions": controls / "QUESTIONS.md",
                              "rubric": controls / "RUBRIC.md", "permissions": permissions,
                              "parser": parser_source, "bridge_source": bridge_source,
                              "bridge": output / "cli-input-bridge.cjs"}[drift]
                    target.write_text("Changed after first trial.\n")
            return result

        with patch.object(pilot, "HERE", controls), patch.object(pilot, "__file__", str(runner)), \
                patch.object(pilot.sandbox_permissions, "__file__", str(permissions)), \
                patch.object(pilot.cli_input, "__file__", str(parser_source)), \
                patch.object(pilot.cli_input, "prepare_bridge", side_effect=fake_bridge), \
                patch.object(pilot, "fingerprint", side_effect=lambda cli: {"digest": state["candidate"]}), \
                patch.object(pilot, "corpus_receipts", side_effect=lambda corpora: {"digest": state["corpora"]}), \
                patch.object(pilot, "capture_tool_contract", create=True,
                             side_effect=lambda cli, remote=False: {"catalog": {"digest": state["toolContract"]}, "localSchemas": {"digest": state["toolContract"]}, "context": state["toolContract"]}), \
                patch.object(pilot, "command", return_value="mock-version\n"), \
                patch.object(pilot, "trial", side_effect=fake_trial), \
                patch.object(pilot.subprocess, "Popen", side_effect=AssertionError("model launched")):
            status = pilot.main(["--check" if check_only else "--run", "--passes", "3", "--cases", "A01", "A02",
                                 "--langchain", str(self.corpus), "--nextjs", str(self.corpus),
                                 "--cli", str(self.cli), "--output-dir", str(output)])
        return status, launches, output, json.loads((output / "report.json").read_text())

    def test_candidate_drift_stops_before_second_trial_and_seals_partial_report(self):
        status, launches, output, report = self.mock_campaign("candidate")
        self.assertEqual(status, 1)
        self.assertEqual(len(launches), 1)
        self.assertEqual(len(report["results"]), 1)
        self.assertFalse(report["candidateUnchanged"])
        self.assertIn("frozen_state_drift:candidate", report["fatalError"])
        self.assertTrue(all(not pair["eligible"] for pair in report["pairs"]))
        manifest = json.loads((output / "manifest.json").read_text())
        for name in ("report.json", "A01-p01-octocode/receipt.json", "A01-p01-octocode/answer.md"):
            self.assertEqual(manifest[name], pilot.digest(output / name))
            self.assertEqual((output / name).stat().st_mode & 0o222, 0)

    def test_every_frozen_input_is_checked_between_trials(self):
        for drift in ("corpora", "runner", "permissions", "questions", "rubric", "missing_rubric", "toolContract",
                      "parser", "bridge_source", "bridge"):
            with self.subTest(drift=drift):
                status, launches, _, report = self.mock_campaign(drift)
                self.assertEqual(status, 1)
                self.assertEqual(len(launches), 1)
                self.assertEqual(len(report["results"]), 1)
                self.assertFalse(report["candidateUnchanged"])
                self.assertIsNotNone(report["fatalError"])

    def test_unchanged_mock_campaign_completes_all_twelve_trials(self):
        status, launches, _, report = self.mock_campaign()
        self.assertEqual(status, 0)
        self.assertEqual(len(launches), 12)
        self.assertEqual(len(report["results"]), 12)
        self.assertTrue(report["candidateUnchanged"])
        self.assertIsNone(report["fatalError"])
        self.assertTrue(all(pair["eligible"] for pair in report["pairs"]))
        self.assertEqual([arm for case, arm, number in launches if case == "A01" and number == 2],
                         ["raw-tools", "octocode"])

    def test_check_exports_frozen_catalog_and_schemas_without_trials(self):
        status, launches, output, report = self.mock_campaign(check_only=True)
        self.assertEqual(status, 0)
        self.assertEqual(launches, [])
        self.assertTrue(report["candidateUnchanged"])
        for filename, key in (("tool-catalog.json", "catalog"), ("local-tool-schemas.json", "localSchemas")):
            self.assertEqual(json.loads((output / filename).read_text()), report["preflight"]["toolContract"][key])
            self.assertEqual((output / filename).stat().st_mode & 0o222, 0)

    def test_corpus_paths_are_explicit_and_can_reuse_frozen_preflight(self):
        from unittest.mock import patch
        receipt = self.root / "preflight.json"
        receipt.write_text(json.dumps({"corpora": {name: {"path": str(self.corpus), "commit": commit}
                                                   for name, commit in pilot.COMMITS.items()}}))
        with patch("builtins.print") as printed:
            self.assertEqual(pilot.main(["--corpus-receipt", str(receipt), "--output-dir", str(self.root / "planned")]), 0)
        plan = json.loads(printed.call_args.args[0])
        self.assertEqual(plan["corpora"]["langchain"]["path"], str(self.corpus.resolve()))
        self.assertEqual(plan["corpora"]["nextjs"]["commit"], pilot.COMMITS["nextjs"])
        self.assertEqual(plan["protocol"], "read-surface-recovery-v10")
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            pilot.main(["--output-dir", str(self.root / "missing-roots")])

    def test_artifacts_cannot_be_written_inside_a_measured_corpus(self):
        from unittest.mock import patch
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            pilot.main(["--langchain", str(self.corpus), "--nextjs", str(self.corpus),
                        "--output-dir", str(self.corpus / "results")])

    def test_environment_expansion_is_rejected_on_both_surfaces(self):
        policy = pilot.Policy("raw-tools", self.cli, [self.corpus])
        for target in ('"$LANGCHAIN/source.py"', '"${NEXTJS}/source.py"',
                       '"$HOME/source.py"', '"$(pwd)/source.py"', '$LANGCHAIN/source.py',
                       '"${LANGCHAIN}/../outside.py"', '"$LANGCHAIN_SUFFIX/source.py"'):
            self.assertIsNotNone(policy.audit(f"rg pass {target}"), target)
        octocode = pilot.Policy("octocode", self.cli, [self.corpus])
        self.assertEqual(octocode.audit(
            f'node {self.cli} tools localFetch --queries \'{{"path":"$LANGCHAIN/source.py"}}\' --compact'),
            "query_outside_corpus")
        self.assertIsNotNone(octocode.audit(
            f'node {self.cli} tools localFetch --queries "{{\\"path\\":\\"$LANGCHAIN/source.py\\"}}" --compact'))

    def test_tool_contract_capture_requires_enabled_schemas_and_bounded_output(self):
        from unittest.mock import patch
        catalog = {"tools": [{"name": name, "availability": {"enabled": True}} for name in pilot.Policy.local_tools]}
        schemas = {"schemas": [{"name": name} for name in pilot.Policy.local_tools]}
        with patch.object(pilot, "command", side_effect=[json.dumps(catalog), json.dumps(schemas), "Canonical context\n"]):
            self.assertEqual(pilot.capture_tool_contract(self.cli)["localSchemas"], schemas)
        catalog["tools"][0]["availability"]["enabled"] = False
        with patch.object(pilot, "command", side_effect=[json.dumps(catalog), json.dumps(schemas), "Canonical context\n"]):
            with self.assertRaisesRegex(RuntimeError, "required_local_tool_unavailable"):
                pilot.capture_tool_contract(self.cli)
        with patch.object(pilot, "command", return_value="x" * 65537):
            with self.assertRaisesRegex(RuntimeError, "tool_contract_output_limit"):
                pilot.capture_tool_contract(self.cli)

    def test_prompt_read_examples_execute_exact_ranges_with_spaced_roots(self):
        import subprocess
        workspace = Path(__file__).resolve().parents[4]
        cli = workspace / "packages/octocode/out/octocode.js"
        with tempfile.TemporaryDirectory(prefix="prompt corpus with spaces ", dir=workspace / ".octocode/tmp") as temp:
            corpus = Path(temp)
            (corpus / "CHOSEN_SUBTREE").mkdir()
            (corpus / "CHOSEN_SUBTREE/source.py").write_text("literal symbol text\n")
            (corpus / "CHOSEN_FILE").write_text("".join(f"fixture line {i}\n" for i in range(1, 41)))
            corpora = {"langchain": corpus, "nextjs": corpus}
            for arm in ("raw-tools",):
                prompt = pilot.prompt_for("A01", arm, "Fixture question", cli, corpora, pilot.Budgets())
                command = next(line for line in prompt.splitlines()
                               if "tools localFetch --queries " in line or line.startswith("sed "))
                with self.subTest(arm=arm):
                    self.assertIsNone(pilot.Policy(arm, cli, [corpus]).audit(command))
                    result = subprocess.run(["/bin/sh", "-c", command],
                                            text=True, capture_output=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                    self.assertIn("fixture line 10", result.stdout)
                    self.assertIn("fixture line 40", result.stdout)
                    self.assertNotIn("fixture line 9\n", result.stdout)

    def test_process_timeout_with_partial_events_no_model(self):
        directory = self.root / "trial"
        directory.mkdir()
        state = self.state(timeout_seconds=0.15)
        child = [sys.executable, "-u", "-c", "import time; print('{\"type\":\"turn.started\"}', flush=True); time.sleep(10)"]
        result = pilot.monitor(child, "prompt", directory, state)
        self.assertIn("process_timeout", result["failures"])
        self.assertIn(b"turn.started", (directory / "events.jsonl").read_bytes())

    def test_question_corpus_notation_is_resolved_before_delivery(self):
        corpora = {"langchain": self.corpus, "nextjs": self.root / "other corpus"}
        question = "Compare `$LANGCHAIN/a.py` and `$NEXTJS/b.ts`; keep $NAME literal."
        expected = f"Compare `{self.corpus}/a.py` and `{corpora['nextjs']}/b.ts`; keep $NAME literal."
        for arm in ("octocode", "raw-tools"):
            prompt = pilot.prompt_for("A12", arm, question, self.cli, corpora, pilot.Budgets())
            self.assertEqual(prompt.split("\nQUESTION A12\n", 1)[1], expected)

    def test_both_surface_examples_discover_complete_paths_with_bounded_output(self):
        import subprocess
        workspace = Path(__file__).resolve().parents[4]
        cli = workspace / "packages/octocode/out/octocode.js"
        with tempfile.TemporaryDirectory(prefix="discovery corpus ' with spaces ", dir=workspace / ".octocode/tmp") as temp:
            corpus = Path(temp)
            subtree = corpus / "CHOSEN_SUBTREE"
            subtree.mkdir()
            names = {f"file{i}.py" for i in range(5)}
            for name in names:
                (subtree / name).write_text(("literal symbol text; literal text (parentheses included) " + "x" * 1000 + "\n") * 20)
            corpora = {"langchain": corpus, "nextjs": corpus}
            for arm in ("raw-tools",):
                with self.subTest(arm=arm):
                    prompt = pilot.prompt_for("A01", arm, "question", cli, corpora, pilot.Budgets())
                    command = next(line for line in prompt.splitlines()
                                   if "tools localSearch --queries " in line or line.startswith("rg "))
                    self.assertIsNone(pilot.Policy(arm, cli, [corpus]).audit(command))
                    result = subprocess.run(["/bin/sh", "-c", command], capture_output=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                    self.assertLessEqual(len(result.stdout), pilot.Budgets().max_output_bytes)
                    if arm == "octocode":
                        data = json.loads(result.stdout)["results"][0]["data"]
                        paths = [item["path"] for item in data["files"]]
                        self.assertFalse(data["pagination"]["hasMore"])
                    else:
                        paths = result.stdout.decode().splitlines()
                    self.assertEqual({Path(path).name for path in paths}, names)


if __name__ == "__main__":
    unittest.main()
