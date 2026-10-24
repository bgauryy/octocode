"""Hermetic catalog-gate and path-denial checks for benchmark preflight."""
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import mcp_probe
from contract_readers import contract_readers

READERS, LOCAL_READERS, CLASSIFIER_READERS = contract_readers()
LOCAL_ALLOWLIST = ','.join(sorted(LOCAL_READERS | {'clasify'}))


def tool(name, classifiers=False):
    properties = {'classification': {'type': 'object', 'properties': {'predicates': {'type': 'array'}}}} if classifiers else {}
    return {'name': name, 'inputSchema': {'properties': {'queries': {'items': {'properties': properties}}}}}


class FakeClient:
    def __init__(self, tools):
        self.tools = tools
        self.calls = []
        self.proc = SimpleNamespace(stdin=SimpleNamespace(write=lambda _: None, flush=lambda: None))

    def call(self, method, params=None):
        self.calls.append((method, params))
        if method == 'initialize': return {'instructions': 'fixture'}
        if method == 'tools/list': return {'tools': self.tools}
        if method == 'tools/call':
            name = params['name']
            row = params['arguments']['queries'][0]
            if name == 'clasify' and row.get('operation') != 'evaluate':
                return {'isError': True, 'content': [{'text': 'invalidInput'}]}
            source = row['source']['query'] if name == 'clasify' else row
            if 'fullContent' in source or any(not isinstance(item, dict) for item in source.get('ranges', [])):
                return {'isError': True, 'content': [{'text': 'invalidInput'}]}
            if name == 'clasify' and params['arguments']['queries'][0]['source']['tool'] == 'ghGetFileContent':
                return {'content': [{'text': 'This source reads through ghGetFileContent, which is disabled in this runtime'}]}
            return {'content': [{'text': 'outsideAllowedRoots'}]}
        raise AssertionError(method)

    def close(self):
        pass


class McpProbeTests(unittest.TestCase):
    def receipt(self, names, classifiers, enabled, selected='', forbidden=None):
        fake = FakeClient([tool(name, name in classifiers) for name in names])
        server = {'env': {'TOOLS_TO_RUN': selected} if selected else {}}
        with patch.object(mcp_probe, 'McpClient', return_value=fake):
            result = mcp_probe.probe(server, enabled, enabled, forbidden=forbidden)
        return result, fake

    def tool_calls(self, fake):
        return [params['name'] for method, params in fake.calls if method == 'tools/call']

    def test_probe_classifier_readers_match_the_contract(self):
        self.assertEqual(mcp_probe.CLASSIFIER_READERS, CLASSIFIER_READERS)

    def test_full_catalog_expects_classifiers_on_exactly_the_contract_classifier_readers(self):
        result, _ = self.receipt(READERS | {'clasify'}, CLASSIFIER_READERS, True)
        self.assertTrue(result['ok'], result['problems'])
        self.assertEqual(set(result['classifiersTools']), CLASSIFIER_READERS)

    def test_every_contract_reader_is_required(self):
        for missing in sorted(READERS):
            with self.subTest(missing=missing):
                result, _ = self.receipt(READERS - {missing}, CLASSIFIER_READERS - {missing}, False)
                self.assertFalse(result['ok'])
                self.assertIn('required read tools missing: ' + missing, result['problems'])

    def test_local_allowlist_expects_local_classifier_readers_and_preserves_denials(self):
        local_classifiers = CLASSIFIER_READERS & LOCAL_READERS
        result, fake = self.receipt(LOCAL_READERS | {'clasify'}, local_classifiers, True,
                                    LOCAL_ALLOWLIST, Path('/private/forbidden.txt'))
        self.assertTrue(result['ok'], result['problems'])
        self.assertEqual(set(result['classifiersTools']), local_classifiers)
        self.assertEqual([row['tool'] for row in result['denials'] if row['denied']],
                         ['localFetch', 'clasify', 'clasify:disabledRemote'])
        self.assertEqual(self.tool_calls(fake), ['localFetch', 'clasify', 'clasify'])

    def test_unexpected_classifiers_field_fails_even_on_selected_reader(self):
        extra = next(iter(sorted(LOCAL_READERS - CLASSIFIER_READERS)))
        result, _ = self.receipt(LOCAL_READERS | {'clasify'}, (CLASSIFIER_READERS & LOCAL_READERS) | {extra},
                                 True, LOCAL_ALLOWLIST)
        self.assertFalse(result['ok'])
        self.assertIn('classifiers schema gate differs from eligible read tools', result['problems'])

    def test_mixed_allowlist_does_not_probe_enabled_remote_reader_for_denial(self):
        names = READERS | {'clasify'}
        result, fake = self.receipt(names, CLASSIFIER_READERS, True, ','.join(sorted(names)),
                                    Path('/private/forbidden.txt'))
        self.assertTrue(result['ok'], result['problems'])
        self.assertEqual([row['tool'] for row in result['denials'] if row['denied']], ['localFetch', 'clasify'])
        self.assertEqual(self.tool_calls(fake), ['localFetch', 'clasify'])

    def test_off_arm_rejects_any_classifiers_field(self):
        selected = ','.join(sorted(LOCAL_READERS))
        result, _ = self.receipt(LOCAL_READERS, {'localSearch'}, False, selected)
        self.assertFalse(result['ok'])
        self.assertIn('classifiers schema gate differs from eligible read tools', result['problems'])
