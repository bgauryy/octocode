"""Arm configuration regressions using fake runners and a fake MCP process."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from contract_readers import contract_readers


HERE = Path(__file__).parent
READERS, LOCAL_READERS, CLASSIFIER_READERS = contract_readers()
GATE_KEYS = ('OCTOCODE_BETA', 'OCTOCODE_CLASSIFICATION_API', 'TOOLS_TO_RUN')
ARMS = ('mcp', 'mcp-clasify', 'local-mcp', 'local-mcp-clasify')


def executable(path, body):
    path.write_text(body)
    path.chmod(0o755)


# Mirrors native's gates: clasify needs OCTOCODE_BETA=true plus a provider credential; reader
# classification (the classifiers schema) needs only the credential.
FAKE_MCP = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
readers = set(json.loads(os.environ['BENCH_TEST_READERS']))
classifier_readers = set(json.loads(os.environ['BENCH_TEST_CLASSIFIER_READERS']))
selected = os.getenv('TOOLS_TO_RUN', '')
names = set(selected.split(',')) if selected else set(readers) | {'clasify'}
credential = bool(os.getenv('OCTOCODE_CLASSIFICATION_API', '').strip())
if not (credential and os.getenv('OCTOCODE_BETA') == 'true'): names.discard('clasify')
snapshot = {key: os.getenv(key) for key in GATE_KEYS}
with Path(os.environ['BENCH_TEST_ENV_LOG']).open('a') as log: log.write(json.dumps(snapshot) + '\\n')
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request: continue
    method = request['method']
    if method == 'initialize': result = {'instructions': 'fixture'}
    elif method == 'tools/list':
        result = {'tools': [{'name': name, 'inputSchema': {'properties': {'classification': {}} if credential and name in classifier_readers else {}}}
                            for name in sorted(names)]}
    elif method == 'tools/call':
        args = request['params']['arguments']
        remote = request['params']['name'] == 'clasify' and args['queries'][0]['source']['tool'] == 'ghGetFileContent'
        result = {'content': [{'type': 'text', 'text': 'ghGetFileContent is disabled in this runtime' if remote else 'outsideAllowedRoots'}]}
    else: raise RuntimeError(method)
    print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}), flush=True)
'''.replace('GATE_KEYS', repr(GATE_KEYS))


class ArmGateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        for name in ('gh', 'pi'):
            executable(self.bin / name, '#!/bin/sh\nexit 0\n')
        executable(self.bin / 'claude',
                   '#!/bin/sh\nprintf \'%s\\n\' \'{"type":"result","subtype":"success","result":"fixture"}\'\n')
        self.fake_mcp = self.root / 'fake_mcp.py'
        executable(self.fake_mcp, FAKE_MCP)
        self.questions = self.root / 'questions'
        self.questions.mkdir()
        (self.questions / 'Q1.md').write_text('## Question\nFixture?\n')
        self.local_repos = self.root / 'repos'
        self.local_repos.mkdir()
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        RUNNER='claude', QUESTION_DIR=str(self.questions), LOCAL_REPOS=str(self.local_repos),
                        OCTO_MCP_CMD=f'{sys.executable} {self.fake_mcp}',
                        OCTOCODE_CLASSIFICATION_API='fixture-key',
                        BENCH_TEST_READERS=json.dumps(sorted(READERS)),
                        BENCH_TEST_CLASSIFIER_READERS=json.dumps(sorted(CLASSIFIER_READERS)),
                        BENCH_TEST_ENV_LOG=str(self.root / 'mcp-env.jsonl'))

    @staticmethod
    def expected_gates(arm):
        """The env each arm must hand the server: beta on, credential blanked only on clasify-off arms."""
        clasify = 'clasify' in arm
        local = LOCAL_READERS | ({'clasify'} if clasify else set())
        return {'OCTOCODE_BETA': 'true',
                'OCTOCODE_CLASSIFICATION_API': None if clasify else '',
                'TOOLS_TO_RUN': local if arm.startswith('local-') else None}

    @staticmethod
    def gates(env):
        gates = {key: env.get(key) for key in GATE_KEYS}
        if gates['TOOLS_TO_RUN'] is not None: gates['TOOLS_TO_RUN'] = set(gates['TOOLS_TO_RUN'].split(','))
        return gates

    def test_run_arm_writes_the_real_clasify_gates_for_each_arm(self):
        campaign = self.root / 'campaign'
        for arm in ARMS:
            with self.subTest(arm=arm):
                result = subprocess.run(['bash', str(HERE / 'run_arm.sh'), str(campaign), arm, '1', 'Q1'],
                                        env=self.env, capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, (result.stderr, result.stdout))
                server = json.loads((campaign / 'runs' / f'{arm}-p1-Q1' / 'mcp.json').read_text())['mcpServers']['octocode']
                self.assertEqual(self.gates(server['env']), self.expected_gates(arm))
                self.assertFalse({key for key in server['env'] if key.startswith('OCTOCODE_')} - set(GATE_KEYS),
                                 'only real gates reach the server')

    def test_preflight_passes_each_arm_with_the_real_gates_and_root_denial(self):
        for arm in ARMS:
            with self.subTest(arm=arm):
                result = subprocess.run(['bash', str(HERE / 'preflight.sh'), arm], env=self.env,
                                        capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, (result.stderr, result.stdout))
                snapshot = json.loads((self.root / 'mcp-env.jsonl').read_text().splitlines()[-1])
                expected = self.expected_gates(arm)
                if expected['OCTOCODE_CLASSIFICATION_API'] is None:
                    expected['OCTOCODE_CLASSIFICATION_API'] = 'fixture-key'
                self.assertEqual(self.gates(snapshot), expected)
                self.assertIn(f'ok    {arm} catalog, feature gates and root denial', result.stdout)
