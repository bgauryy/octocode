"""Current benchmark callers execute real local requests without provider inference."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import tempfile
from threading import Thread
import unittest
from unittest.mock import patch

from local_model_benchmark import capture_schemas, native_call
from mcp_probe import LOCAL_READERS, probe


ROOT = Path(__file__).resolve().parents[3]


class ProviderPing(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200 if self.path == '/v1/models' else 404)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"models":[]}')

    def log_message(self, *_args):
        pass


class CurrentConsumerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='octocode-benchmark-contract-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.lab = self.root / 'lab'
        self.lab.mkdir()
        provider = ThreadingHTTPServer(('127.0.0.1', 0), ProviderPing)
        self.addCleanup(provider.server_close)
        self.addCleanup(provider.shutdown)
        Thread(target=provider.serve_forever, kwargs={'poll_interval': 0.01}, daemon=True).start()
        resolver = ROOT / 'packages/octocode-native/bin/resolve-binary.cjs'
        self.binary = Path(subprocess.check_output(
            ['node', '-e', 'process.stdout.write(require(process.argv[1]).resolveNativeBinaryPath())', str(resolver)],
            text=True, timeout=10))
        env = {'PATH': os.environ['PATH'], 'HOME': str(self.root), 'OCTOCODE_HOME': str(self.root / 'octocode'),
               'OCTOCODE_ENABLE_LOCAL': 'true', 'OCTOCODE_STORAGE_MODE': 'persistent',
               'WORKSPACE_ROOT': str(self.lab), 'ALLOWED_PATHS': str(self.lab),
               'OCTOCODE_CLASSIFICATION_API': 'fixture-key', 'OCTOCODE_CLASSIFICATION_API_HOST': f'http://127.0.0.1:{provider.server_port}',
               'OCTOCODE_BETA': 'false', 'NO_PROXY': '127.0.0.1,localhost',
               'HTTP_PROXY': 'http://127.0.0.1:9', 'HTTPS_PROXY': 'http://127.0.0.1:9'}
        self.environment = patch.dict(os.environ, env, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def test_local_model_schema_and_cursor_walk_preserve_every_line(self):
        tools = capture_schemas(self.binary, self.root / 'schema')
        self.assertEqual({tool['function']['name'] for tool in tools}, LOCAL_READERS)
        lines = [f'line {i}: evidence 界😀' for i in range(1, 301)]
        (self.lab / 'source.txt').write_text('\n'.join(lines) + '\n')
        pending = [{'queries': [{'path': 'source.txt'}], 'responseChars': 900}]
        seen, actual = set(), {}
        while pending:
            self.assertLess(len(seen), 100, 'cursor walk terminates')
            record = native_call(self.binary, 'localFetch', pending.pop(), self.lab)
            self.assertTrue(record['ok'], record)
            page = record['result']
            calls = [page['responsePagination']['next']] if page.get('responsePagination', {}).get('next') else []
            for row in page['results']:
                for line in row['data'].get('content', '').splitlines():
                    number, text = line.split('\t', 1)
                    self.assertNotIn(int(number), actual)
                    actual[int(number)] = text
                calls.extend(row['data'].get('next', {}).values())
            for call in calls:
                self.assertEqual(call['kind'], 'continue')
                self.assertEqual(call['tool'], 'localFetch')
                self.assertEqual(set(call['input']), {'cursor'})
                cursor = call['input']['cursor']
                self.assertNotIn(cursor, seen)
                seen.add(cursor)
                pending.append(call['input'])
        self.assertTrue(seen, 'the fixture must exercise replay')
        self.assertEqual(actual, dict(enumerate(lines, 1)))

    def test_preflight_proves_scope_denial_instead_of_input_rejection(self):
        forbidden = self.root / 'private.txt'
        forbidden.write_text('private evaluator evidence')
        selected = sorted(LOCAL_READERS | {'clasify'})
        server = {'command': str(self.binary), 'args': ['mcp'], 'env': {'TOOLS_TO_RUN': ','.join(selected)}}
        receipt = probe(server, True, True, cwd=self.lab, forbidden=forbidden)
        self.assertTrue(receipt['ok'], json.dumps(receipt['problems']))
        self.assertEqual([row['tool'] for row in receipt['denials']], ['localFetch', 'clasify', 'clasify:disabledRemote'])
        for denial in receipt['denials'][:2]:
            row = denial['response']['structuredContent']['results'][0]
            self.assertEqual(row['data']['errorCode'], 'outsideAllowedRoots')
        remote = receipt['denials'][2]['response']['structuredContent']
        self.assertIn('ghGetFileContent, which is disabled in this runtime', remote['error'])
        self.assertNotIn('private evaluator evidence', json.dumps(receipt['denials']))


if __name__ == '__main__':
    unittest.main()
