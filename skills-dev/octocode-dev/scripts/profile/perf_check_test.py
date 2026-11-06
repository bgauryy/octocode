"""Hermetic acceptance tests for the local performance comparison command."""
import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location('perf_check', Path(__file__).with_name('perf_check.py'))
PERF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PERF)


class PerfCheckTests(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix='octocode-perf-check-test-')
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        self.binary = self.root / 'octocode'
        self.binary.touch()
        self.baseline = self.root / 'baseline.json'
        self.stored = {'corpus': PERF.CORPUS_REV, 'platforms': {
            'test': {'rustc': 'rustc test', 'flows': {'plain': 100_000_000, 'remote': 100_000_000}}}}
        self.write_baseline()
        self.row = {'instr': 100_000_000, 'max': 105_000_000, 'peak': 10_000,
                    'wall': 0.1, 'bytes': 10, 'errors': [], 'reps': 3, 'instructionSamples': 3}
        flows = [dict(id=name, tool='localFetch', query={}, gate=gate, stub=stub)
                 for name, gate, stub in [('plain', True, False), ('remote', True, True), ('costly', False, False)]]
        patches = {'BASELINE': self.baseline, 'FLOWS': flows, 'platform_key': lambda: 'test',
                   'toolchain': lambda: 'rustc test', 'can_count': lambda: None,
                   'corpus': lambda _: self.root, 'count': Mock(side_effect=lambda *args: dict(self.row)),
                   'stub_env': lambda _: {}, 'with_stub': lambda query, _: query,
                   'stub_server': Mock(return_value=(Mock(), 'http://stub'))}
        for name, value in patches.items():
            mocked = patch.object(PERF, name, value)
            mocked.start()
            self.addCleanup(mocked.stop)

    def write_baseline(self):
        self.baseline.write_text(json.dumps(self.stored))

    def run_check(self, *args):
        output = io.StringIO()
        with patch('sys.argv', ['perf_check', '--bin', str(self.binary), *args]), \
                contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            try:
                status = PERF.main()
            except SystemExit as error:
                status = error.code if isinstance(error.code, int) else 1
        return status, output.getvalue()

    def test_requested_flow_typos_and_non_gate_flows_are_errors(self):
        for selection in ['', 'missing', 'costly', 'plain,missing']:
            with self.subTest(selection=selection):
                self.assertEqual(self.run_check('--flows', selection)[0], 2)
        PERF.count.assert_not_called()

    def test_repetitions_must_be_positive(self):
        for repetitions in ['0', '-1']:
            self.assertEqual(self.run_check('--reps', repetitions)[0], 2)

    def test_unavailable_counters_are_unverified(self):
        with patch.object(PERF, 'can_count', return_value='no hardware counter'):
            self.assertEqual(self.run_check()[0], 3)

    def test_unknown_toolchain_cannot_write_a_baseline(self):
        original = self.baseline.read_bytes()
        with patch.object(PERF, 'toolchain', return_value=''):
            self.assertEqual(self.run_check('--write')[0], 3)
        self.assertEqual(self.baseline.read_bytes(), original)

    def test_missing_or_incompatible_baseline_is_unverified(self):
        for change in ['platform', 'toolchain', 'corpus', 'flow']:
            with self.subTest(change=change):
                stored = json.loads(json.dumps(self.stored))
                if change == 'platform': stored['platforms'] = {}
                if change == 'toolchain': stored['platforms']['test']['rustc'] = 'old rustc'
                if change == 'corpus': stored['corpus'] = 'old corpus'
                if change == 'flow': stored['platforms']['test']['flows'].pop('remote')
                self.baseline.write_text(json.dumps(stored))
                self.assertEqual(self.run_check()[0], 3)

    def test_a_stub_failure_cannot_shrink_the_requested_denominator(self):
        with patch.object(PERF, 'stub_server', return_value=(None, 'unavailable')):
            status, output = self.run_check()
        self.assertEqual(status, 3)
        self.assertIn('remote', output)
        self.assertIn('1/2', output)

    def test_invalid_instruction_counts_never_pass_or_overwrite_a_baseline(self):
        original = self.baseline.read_bytes()
        for bad in [None, 0, -1, float('nan'), float('inf'), True]:
            for write in [False, True]:
                with self.subTest(count=bad, write=write):
                    self.row['instr'] = bad
                    self.assertEqual(self.run_check(*(['--write'] if write else []))[0], 3)
                    self.assertEqual(self.baseline.read_bytes(), original)

    def test_missing_repetition_counters_are_not_a_cheaper_measurement(self):
        self.row['instructionSamples'] = 2
        self.assertEqual(self.run_check()[0], 3)

    def test_shared_counter_retains_missing_sample_denominator(self):
        profile = __import__('profile')
        server = Mock()
        measured = {'instr': 100, 'peak': 200, 'wall': 0.1, 'bytes': 10}
        server.call.side_effect = [{}, measured, {}, {**measured, 'instr': None}, {}, measured]
        with patch.object(profile, 'Mcp', return_value=server):
            counted = profile.count(self.binary, {'tool': 'localFetch', 'phase': 'first'}, {}, self.root, {}, 3)
        self.assertEqual(counted['reps'], 3)
        self.assertEqual(counted['instructionSamples'], 2)
        self.assertEqual(counted['peakSamples'], 3)
        self.assertEqual((counted['instr'], counted['max']), (100, 100))
        self.assertEqual(counted['samples'], [measured, {**measured, 'instr': None}, measured])

    def test_invalid_baseline_counts_cannot_compare(self):
        for bad in [0, -1, float('nan'), float('inf'), True, None]:
            self.stored['platforms']['test']['flows']['plain'] = bad
            self.write_baseline()
            self.assertEqual(self.run_check()[0], 3)

    def test_valid_same_cheaper_and_regressed_results(self):
        for current, status in [(100_000_000, 0), (50_000_000, 0), (150_000_000, 1)]:
            self.row.update(instr=current, max=current)
            actual, output = self.run_check()
            self.assertEqual(actual, status)
            self.assertIn('2/2', output)

    def test_tool_errors_are_failed_not_measurements(self):
        self.row['errors'] = ['invalidInput']
        self.assertEqual(self.run_check()[0], 1)

    def test_writing_a_subset_under_a_new_toolchain_does_not_relabel_old_rows(self):
        self.stored['platforms']['test']['rustc'] = 'old rustc'
        self.write_baseline()
        self.assertEqual(self.run_check('--flows', 'plain', '--write')[0], 0)
        written = json.loads(self.baseline.read_text())
        self.assertEqual(set(written['platforms']['test']['flows']), {'plain'})

    def test_a_new_corpus_does_not_relabel_another_platform(self):
        self.stored['corpus'] = 'old corpus'
        self.stored['platforms']['other'] = self.stored['platforms']['test']
        self.write_baseline()
        self.assertEqual(self.run_check('--write')[0], 0)
        self.assertEqual(set(json.loads(self.baseline.read_text())['platforms']), {'test'})


class McpReplyTests(unittest.TestCase):
    def reply(self, result):
        harness = __import__('harness')
        server = object.__new__(harness.Mcp)
        server.pid = 42
        server.rpc = Mock(return_value={'result': result})
        with patch.object(harness, 'DARWIN', True), patch.object(harness, 'rusage', side_effect=[
                {'instr': 100, 'peak': 200}, {'instr': 150, 'peak': 250}]):
            return server.call('localFetch', {'queries': [{'path': 'a'}]})

    def test_text_only_mcp_errors_are_failed_measurements(self):
        result = {'isError': True, 'content': [{'type': 'text', 'text': 'provider unavailable'}]}
        reply = self.reply(result)
        self.assertEqual(reply['error'], 'provider unavailable')
        self.assertIsNone(reply['content'])
        self.assertEqual((reply['instr'], reply['peak']), (50, 250))
        self.assertEqual(reply['bytes'], len('provider unavailable'))

    def test_structured_error_codes_and_evidence_are_preserved(self):
        content = {'results': [{'index': 0, 'status': 'error', 'data': {
            'errorCode': 'providerUnavailable', 'error': 'provider detail'}}]}
        for envelope in [{'structuredContent': content}, {'content': [{'text': json.dumps(content)}]}]:
            with self.subTest(envelope=envelope):
                reply = self.reply({'isError': True, **envelope})
                self.assertEqual(reply['error'], 'providerUnavailable')
                self.assertEqual(reply['content'], content)

    def test_error_flag_alone_is_not_a_happy_envelope(self):
        for content in [None, {}, {'results': []}]:
            with self.subTest(content=content):
                self.assertTrue(self.reply({'isError': True, 'structuredContent': content})['error'])

    def test_happy_structured_and_json_text_answers_keep_their_counters(self):
        content = {'results': [{'index': 0, 'data': {'content': 'evidence'}}]}
        for envelope in [{'structuredContent': content}, {'content': [{'text': json.dumps(content)}]}]:
            with self.subTest(envelope=envelope):
                reply = self.reply(envelope)
                self.assertIsNone(reply['error'])
                self.assertEqual(reply['content'], content)
                self.assertEqual((reply['instr'], reply['peak']), (50, 250))

    def test_unparseable_success_is_not_an_empty_successful_measurement(self):
        reply = self.reply({'content': [{'text': 'not a JSON tool answer'}]})
        self.assertEqual(reply['error'], 'noJson')
        self.assertIsNone(reply['content'])

    def test_partial_coverage_survives_for_the_flow_readiness_gate(self):
        profile = __import__('profile')
        content = {'results': [{'index': 0, 'data': {}}, {'index': 1, 'data': {
            'isPartial': True, 'partialReasons': ['buildScriptsOff']}}]}
        reply = self.reply({'structuredContent': content})
        self.assertEqual(reply['content'], content)
        self.assertIsNone(reply['error'])
        with self.assertRaisesRegex(SystemExit, 'row 1.*buildScriptsOff'):
            profile.require_server_coverage({'id': 'lsp.test', 'server': 'rust-analyzer'}, reply)


if __name__ == '__main__':
    unittest.main()
