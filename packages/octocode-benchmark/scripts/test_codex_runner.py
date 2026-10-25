#!/usr/bin/env python3
"""Codex transcript and isolation regressions without paid model/provider calls."""
import json, os, subprocess, sys, tempfile, unittest
from pathlib import Path
from codex_runner import command, DISABLED_FEATURES, semantic_gates, main as run_codex
from transcript_to_log import parse_codex
from contract_readers import has_classification
from render_question import prepare_question
from unittest.mock import patch

class CodexRunnerTests(unittest.TestCase):
    def parse(self, events, ready=True):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)
            (root/'transcript.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
            (root/'runner.json').write_text('{"model":"gpt-5.6-terra"}')
            (root/'preflight.json').write_text(json.dumps({'ok':ready}))
            return parse_codex(root/'transcript.jsonl',root)
    def test_content_errors_usage_and_missing_timing_are_retained(self):
        item={'id':'a','type':'mcp_tool_call','server':'octocode','tool':'localFetch','arguments':{'queries':[{'path':'a'}]}}
        result={**item,'status':'completed','result':{'content':[{'type':'text','text':'one\ntwo'}],'isError':True}}
        uses,order,results,final,problems=self.parse([
            {'type':'item.started','item':item,'_t':10},
            {'type':'item.completed','item':result,'_t':12},
            {'type':'item.completed','item':{'type':'agent_message','text':'Cannot read.'}},
            {'type':'turn.completed','usage':{'input_tokens':100,'cached_input_tokens':25,'output_tokens':7}},
            {'type':'bench_end','start':5,'_t':15,'exit':0}])
        self.assertEqual(order,['a']);self.assertEqual(results['a'],('one\ntwo',True))
        self.assertEqual(final['usage']['input_tokens'],100)
        self.assertEqual(final['usage']['cached_input_tokens'],25)
        self.assertNotIn('cache_read_input_tokens',final['usage'])
        self.assertEqual(final['observed_tool_duration_ms'],2000)
        self.assertIsNone(final['duration_api_ms']);self.assertIsNone(final['num_turns'])
        self.assertEqual(final['subtype'],'success');self.assertEqual(problems,[])
    def test_timeout_and_incomplete_calls_do_not_become_success(self):
        _,order,results,final,problems=self.parse([
            {'type':'item.started','item':{'id':'x','type':'mcp_tool_call','server':'octocode','tool':'localSearch','arguments':{}},'_t':1},
            {'type':'bench_end','start':0,'_t':600,'exit':-15,'reason':'timeout'}])
        self.assertEqual(order,['x']);self.assertEqual(results,{})
        self.assertEqual(final['subtype'],'timeout');self.assertIn('Codex usage missing',problems)
    def test_extra_tools_and_missing_preflight_are_invalid(self):
        _,_,_,_,problems=self.parse([{'type':'item.completed','item':{'type':'command_execution'}}],False)
        self.assertIn('unexpected built-in tool: command_execution',problems)
        self.assertIn('MCP preflight unavailable or failed',problems)
    def test_command_has_only_explicit_server_and_no_credentials(self):
        with patch.dict(os.environ,{'OCTOCODE_CLASSIFICATION_API':'secret-value'}):
            cmd=command('gpt-5.6-terra',Path('/tmp/lab'),{'command':'node','args':['/build/index.js'],'env':{'OCTOCODE_BETA':'true'}})
        for flag in ('--ignore-user-config','--ignore-rules','--ephemeral','--no-daemon'): self.assertIn(flag,cmd)
        for feature in DISABLED_FEATURES: self.assertIn(feature,cmd)
        self.assertTrue(any(x.startswith('mcp_servers=') for x in cmd))
        self.assertFalse(any('secret-value' in x for x in cmd))
        self.assertTrue(any('env_vars' in x and '"OCTOCODE_CLASSIFICATION_API"' in x for x in cmd))
    def test_recursive_schema_gate(self):
        self.assertTrue(has_classification({'properties':{'queries':{'items':{'properties':{'classification':{'type':'object'}}}}}}))
        self.assertFalse(has_classification({'description':'classifiers are optional'}))
        self.assertFalse(has_classification({'properties':{'queries':{'items':{'properties':{'goals':{'type':'array'}}}}}}))
    def test_clasify_and_reader_classifiers_need_a_credential_independent_of_beta(self):
        inherited = {'OCTOCODE_CLASSIFICATION_API': 'test-credential'}
        for beta, expected in [('false', (True, True)), ('true', (True, True))]:
            with self.subTest(beta=beta):
                self.assertEqual(semantic_gates({'env': {'OCTOCODE_BETA': beta}}, inherited), expected)
        for restriction in [{'TOOLS_TO_RUN':'localSearch,localFetch'}, {'DISABLE_TOOLS':'clasify'}]:
            with self.subTest(restriction=restriction):
                server = {'env': {'OCTOCODE_BETA':'true', **restriction}}
                self.assertEqual(semantic_gates(server, inherited), (False,True))
        for blank in ['', '  ']:
            with self.subTest(credential=blank):
                server = {'env': {'OCTOCODE_BETA':'true','OCTOCODE_CLASSIFICATION_API':blank}}
                self.assertEqual(semantic_gates(server, inherited), (False,False))
class CodexBudgetTests(unittest.TestCase):
    def test_stdout_eof_before_successful_exit_does_not_signal_worker(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'prompt.md').write_text('Research')
            (root/'mcp.json').write_text(json.dumps({'mcpServers':{'octocode':{'command':'unused','env':{'OCTOCODE_CLASSIFICATION_API':''}}}}))
            # A real child closes stdout before completing host cleanup. The old
            # runner immediately sent SIGTERM and lost the successful exit.
            script='import os,time; os.close(1); time.sleep(0.1)'
            argv=['runner','--run',str(root),'--prompt',str(root/'prompt.md'),'--mcp-config',str(root/'mcp.json'),'--model','test','--timeout','5']
            with patch('sys.argv',argv), patch('codex_runner.probe',return_value={'ok':True}), patch('codex_runner.command',return_value=[sys.executable,'-c',script]), patch.dict(os.environ,{'BENCH_CLASSIFICATION_TRACE':'0'}):
                self.assertEqual(run_codex(),0)
            end=json.loads((root/'transcript.jsonl').read_text().splitlines()[-1])
            self.assertEqual(end['exit'],0)
            self.assertIsNone(end['reason'])

    def test_unexpected_runner_error_keeps_full_traceback(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)
            result=subprocess.run([sys.executable,str(Path(__file__).with_name('codex_runner.py')),
                '--run',str(root),'--prompt',str(root/'prompt.md'),'--mcp-config',str(root/'missing.json'),
                '--model','test'],capture_output=True,text=True,timeout=5)
            self.assertEqual(result.returncode,2)
            end=json.loads((root/'transcript.jsonl').read_text().splitlines()[-1])
            trace=Path(end['traceback_path']).read_text()
            self.assertIn('Traceback (most recent call last)',trace)
            self.assertIn('FileNotFoundError',trace)
            self.assertIn('server=json.loads',trace)

    def test_tool_budget_stops_process_and_keeps_attempted_call(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'prompt.md').write_text('Research')
            (root/'mcp.json').write_text(json.dumps({'mcpServers':{'octocode':{'command':'unused','env':{'OCTOCODE_CLASSIFICATION_API':''}}}}))
            events=[{'type':'item.started','item':{'id':str(i),'type':'mcp_tool_call','server':'octocode','tool':'localFetch','arguments':{}}} for i in range(2)]
            script='import time; '+ '; '.join('print('+repr(json.dumps(e))+',flush=True)' for e in events)+'; time.sleep(60)'
            argv=['runner','--run',str(root),'--prompt',str(root/'prompt.md'),'--mcp-config',str(root/'mcp.json'),'--model','test','--timeout','5','--max-tool-calls','1']
            with patch('sys.argv',argv), patch('codex_runner.probe',return_value={'ok':True}), patch('codex_runner.command',return_value=[sys.executable,'-c',script]), patch.dict(os.environ,{'BENCH_CLASSIFICATION_TRACE':'0'}):
                self.assertEqual(run_codex(),2)
            rows=[json.loads(l) for l in (root/'transcript.jsonl').read_text().splitlines()]
            self.assertEqual(rows[-1]['reason'],'tool_call_cap')
            self.assertEqual(rows[-1]['tool_calls'],2)
            self.assertEqual(len([r for r in rows if r['type']=='item.started']),2)

class CorpusIsolationTests(unittest.TestCase):
    def test_only_selected_tracked_corpus_enters_solver_lab(self):
        with tempfile.TemporaryDirectory() as td:
            base=Path(td); source=base/'sources'/'example'; source.mkdir(parents=True)
            (source/'code.py').write_text('print(1)')
            (source/'private-answer.md').write_text('must never be copied')
            manifest=base/'repositories.json'
            manifest.write_text(json.dumps({'repositories':[{'id':'example','sourceRoot':str(source),'commit':'abc'}]}))
            def git(cmd, **kwargs):
                if 'rev-parse' in cmd: return b'abc\n'
                if 'ls-files' in cmd: return b'code.py\0'
                return b''
            with patch('render_question.subprocess.check_output',side_effect=git):
                text, receipt=prepare_question('Read {{REPO:example}}',manifest,base/'lab',base/'sources')
            self.assertEqual((base/'lab/example/code.py').read_text(),'print(1)')
            self.assertFalse((base/'lab/example/private-answer.md').exists())
            self.assertEqual(text,'Read '+str(base/'lab/example'))
            self.assertEqual(receipt[0]['sourceRoot'],str(source.resolve()))
    def test_external_scope_mapping_keeps_question_free_of_metadata(self):
        with tempfile.TemporaryDirectory() as td:
            base = Path(td)
            source = base / 'sources/example'
            source.mkdir(parents=True)
            (source / 'code.py').write_text('print(1)')
            manifest = base / 'repositories.json'
            manifest.write_text(json.dumps({'repositories': [{'id': 'example', 'sourceRoot': str(source), 'commit': 'abc'}],
                                            'questionRepositories': {'Q37': ['example']}}))
            def git(cmd, **kwargs):
                if 'rev-parse' in cmd: return b'abc\n'
                if 'ls-files' in cmd: return b'code.py\0'
                return b''
            question = '# Goal\nExplain the behavior.\n'
            with patch.dict(os.environ, {'BENCH_QUESTION_ID': 'Q37'}), patch('render_question.subprocess.check_output', side_effect=git):
                text, receipt = prepare_question(question, manifest, base / 'lab', base / 'sources')
            self.assertTrue(text.startswith(question))
            self.assertIn(str(base / 'lab/example'), text)
            self.assertNotIn(str(source), text)
            self.assertEqual(len(receipt), 1)

    def test_missing_template_map_fails_closed(self):
        with self.assertRaises(ValueError):
            prepare_question('Read {{REPO:example}}',None,Path('/tmp/not-used'),Path('/tmp'))

if __name__=='__main__': unittest.main()
