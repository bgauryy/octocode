"""Readiness checks never measure incomplete language-server coverage as success."""
import importlib.util
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location('tool_profile', Path(__file__).with_name('profile.py'))
PROFILE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROFILE)


def answer(*rows):
    return {'content': {'results': [dict(index=index, data=row) for index, row in enumerate(rows)]}}


def partial(*reasons):
    return {'isPartial': True, 'partialReasons': list(reasons)}


class LanguageServerReadinessTests(unittest.TestCase):
    def setUp(self):
        self.flow = dict(id='lspSearch', tool='lspSearch', server=True)
        self.server = Mock()
        self.sleep = patch.object(PROFILE.time, 'sleep')
        self.sleep.start()
        self.addCleanup(self.sleep.stop)

    def test_permanent_coverage_is_refused_without_retrying(self):
        self.server.call.return_value = answer(partial('scanOnlyImplementations', 'buildScriptsOff'))
        with self.assertRaisesRegex(SystemExit, 'unverified.*scanOnlyImplementations.*buildScriptsOff'):
            PROFILE.settle(self.server, self.flow, {})
        self.server.call.assert_called_once()

    def test_a_later_partial_row_cannot_hide_behind_a_complete_first_row(self):
        self.server.call.return_value = answer({}, partial('buildScriptsOff'))
        with self.assertRaisesRegex(SystemExit, 'row 1.*buildScriptsOff'):
            PROFILE.settle(self.server, self.flow, {})
        self.server.call.assert_called_once()

    def test_only_indexing_may_retry_and_then_return_the_complete_answer(self):
        ready = answer({})
        self.server.call.side_effect = [answer(partial('languageServerIndexing')), ready]
        self.assertIs(PROFILE.settle(self.server, self.flow, {}), ready)
        self.assertEqual(self.server.call.call_count, 2)

    def test_indexing_does_not_make_a_permanent_reason_retryable(self):
        self.server.call.return_value = answer(partial('languageServerIndexing', 'buildScriptsOff'))
        with self.assertRaisesRegex(SystemExit, 'buildScriptsOff'):
            PROFILE.settle(self.server, self.flow, {})
        self.server.call.assert_called_once()

    def test_unknown_partial_coverage_is_refused(self):
        self.server.call.return_value = answer(partial())
        with self.assertRaisesRegex(SystemExit, 'unverified.*unspecified'):
            PROFILE.settle(self.server, self.flow, {})

    def test_indexing_that_never_settles_is_not_a_successful_measurement(self):
        self.server.call.return_value = answer(partial('languageServerIndexing'))
        with self.assertRaisesRegex(SystemExit, 'unverified.*languageServerIndexing'):
            PROFILE.settle(self.server, self.flow, {})

    def test_the_measured_repeat_must_still_have_complete_coverage(self):
        ready = {**answer({}), 'instr': 100, 'peak': 200, 'wall': 0.1, 'bytes': 10}
        incomplete = {**ready, **answer(partial('buildScriptsOff'))}
        self.server.call.side_effect = [ready, ready, incomplete]
        with patch.object(PROFILE, 'Mcp', return_value=self.server), \
                self.assertRaisesRegex(SystemExit, 'unverified.*buildScriptsOff'):
            PROFILE.count(Path('unused'), {**self.flow, 'phase': 'repeat'}, {}, Path('.'), 1)
        self.server.close.assert_called_once()


if __name__ == '__main__':
    unittest.main()
