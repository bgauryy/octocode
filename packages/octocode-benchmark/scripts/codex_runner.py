#!/usr/bin/env python3
"""Fresh Codex exec runner, restricted to one explicit Octocode MCP server.

Codex JSONL does not expose internal sampling turns or precise model time. The hard
budget is tool calls plus wall time; both are named honestly in receipts.
"""
from __future__ import annotations
import argparse, hashlib, json, os, queue, signal, subprocess, sys, tempfile, threading, time, traceback
from contextlib import contextmanager
from pathlib import Path
from mcp_probe import probe
from render_question import prepare_question

DISABLED_FEATURES = ('shell_tool','unified_exec','view_image','apps','plugins','memories',
    'multi_agent','multi_agent_v2','browser_use','browser_use_external','computer_use',
    'image_generation','goals','sleep_tool','hooks','skill_search','workspace_dependencies',
    'code_mode_host','unbounded_connection_retries','shell_snapshot')
ENV_ALLOW = ('PATH','LANG','LC_ALL','TMPDIR','CODEX_HOME','OPENAI_API_KEY','GH_TOKEN','GITHUB_TOKEN',
    'OCTOCODE_CLASSIFICATION_API','OCTOCODE_CLASSIFICATION_MODEL','OCTOCODE_CLASSIFICATION_API_HOST',
    'OCTOCODE_LSP_CONFIG','OCTOCODE_LSP_CACHE_DIR','SSL_CERT_FILE','SSL_CERT_DIR','NODE_EXTRA_CA_CERTS')


def toml(value):
    if isinstance(value,dict): return '{'+', '.join(json.dumps(k)+' = '+toml(v) for k,v in value.items())+'}'
    if isinstance(value,list): return '['+', '.join(toml(v) for v in value)+']'
    return json.dumps(value)


def command(model, lab, server):
    cmd=['codex','--no-daemon','exec','--ignore-user-config','--ignore-rules','--ephemeral',
         '--skip-git-repo-check','--strict-config','--json','--model',model,'-C',str(lab),'-s','read-only']
    for name in DISABLED_FEATURES: cmd+=['--disable',name]
    cmd+=['--enable','skip_host_skill_discovery']
    config={'project_doc_max_bytes':0,'skills.include_instructions':False,'skills.bundled.enabled':False,
        'web_search':'disabled','features.code_mode':{'enabled':False,'direct_only_tool_namespaces':['mcp__octocode']}, 'tools.update_plan.enabled':False,'tools.experimental_request_user_input.enabled':False,
        'model_reasoning_effort':os.environ.get('RUNNER_REASONING','medium'),'developer_instructions':'',
        'tool_output_token_limit':100000,'suppress_unstable_features_warning':True,
        'mcp_servers':{'octocode':{**{k:v for k,v in server.items() if k in ('command','args','env')},
            'env_vars':list(ENV_ALLOW),'required':True,'tool_input_schema_max_bytes':100000,'startup_timeout_sec':60,'tool_timeout_sec':120}}}
    for key,value in config.items(): cmd+=['-c',key+'='+toml(value)]
    return cmd+['-']


def terminate(proc):
    if proc.poll() is None:
        os.killpg(proc.pid,signal.SIGTERM)
        try: proc.wait(timeout=3)
        except subprocess.TimeoutExpired: os.killpg(proc.pid,signal.SIGKILL); proc.wait()


def semantic_gates(server, env):
    """Expected gates for explicitly configured benchmark arms, independently."""
    effective = {**env, **server.get('env', {})}
    credential = bool(effective.get('OCTOCODE_CLASSIFICATION_API', '').strip())
    selected = {name.strip() for name in effective.get('TOOLS_TO_RUN', '').split(',') if name.strip()}
    disabled = {name.strip() for name in effective.get('DISABLE_TOOLS', '').split(',') if name.strip()}
    clasify = (credential and effective.get('OCTOCODE_BETA') == 'true'
               and (not selected or 'clasify' in selected) and 'clasify' not in disabled)
    classifiers = credential and effective.get('OCTOCODE_RANKING_ENABLED') == 'true'
    return clasify, classifiers


@contextmanager
def classification_trace(server, env, run):
    enabled = any(semantic_gates(server, env))
    if not enabled or os.environ.get("BENCH_CLASSIFICATION_TRACE") != "1":
        yield None
        return
    proxy = Path(__file__).with_name("classification_proxy.mjs")
    with (run / "classification-proxy-stderr.log").open("w") as err:
        proc = subprocess.Popen(["node", str(proxy), "--log", str(run / "classification-http.jsonl")],
            env=env, stdout=subprocess.PIPE, stderr=err, text=True, start_new_session=True)
        messages = queue.Queue()
        threading.Thread(target=lambda: messages.put(proc.stdout.readline()), daemon=True).start()
        try:
            message = messages.get(timeout=10)
            if not message: raise RuntimeError("Classification tracing proxy did not start")
            yield json.loads(message)["url"]
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGTERM)
                try: proc.wait(timeout=35)
                except subprocess.TimeoutExpired: terminate(proc)
            proc.stdout.close()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--run',type=Path,required=True); ap.add_argument('--prompt',type=Path,required=True)
    ap.add_argument('--model',required=True); ap.add_argument('--mcp-config',type=Path,required=True)
    ap.add_argument('--timeout',type=int,default=600); ap.add_argument('--max-tool-calls',type=int,default=30)
    args=ap.parse_args(); run=args.run.resolve(); run.mkdir(parents=True,exist_ok=True)
    server=json.loads(args.mcp_config.read_text())['mcpServers']['octocode']
    env={k:v for k,v in os.environ.items() if k in ENV_ALLOW}
    # HOME is retained for authentication only; shell, files, apps, skills and plugins are disabled.
    if 'HOME' in os.environ: env['HOME']=os.environ['HOME']
    errors=[]; calls=0; started=time.time(); reason=None
    with tempfile.TemporaryDirectory(prefix='octocode-bench-codex-') as folder, classification_trace(server,env,run) as trace_url:
        base=Path(folder).resolve(); lab=base/'lab'; lab.mkdir(); home=base/'octocode'; home.mkdir()
        if os.environ.get('BENCH_RATE_LIMIT_DIR'):
            rate = Path(os.environ['BENCH_RATE_LIMIT_DIR']).resolve()
            rate.mkdir(parents=True,exist_ok=True)
            (home/'tmp').mkdir()
            (home/'tmp'/'ratelimit').symlink_to(rate,target_is_directory=True)
        sentinel=base/'evaluator-canary.txt'; sentinel.write_text('BENCHMARK_PRIVATE_CANARY_DO_NOT_READ')
        server['env']={**server.get('env',{}),'OCTOCODE_HOME':str(home),'ALLOWED_PATHS':'','OCTOCODE_ENABLE_LOCAL':'true',
            'OCTOCODE_ENABLE_STATS':'true','OCTOCODE_STORAGE_MODE':'persistent',
            'OCTOCODE_TRUST_PROJECT_LSP_CONFIG':'false','OCTOCODE_LSP_AUTO_INSTALL':'off','OCTOCODE_LSP_PREWARM':'off'}
        if trace_url: server['env']['OCTOCODE_CLASSIFICATION_API_HOST']=trace_url
        prompt, corpus = prepare_question(args.prompt.read_text(), os.environ.get('QUESTION_REPO_MAP'),
            lab/'repos', os.environ.get('LOCAL_REPOS', server['env'].get('WORKSPACE_ROOT',str(lab))))
        if corpus:
            server['env']['WORKSPACE_ROOT']=str(lab/'repos')
            (run/'corpus.json').write_text(json.dumps(corpus,indent=1))
            (run/'prompt.template.md').write_bytes(args.prompt.read_bytes())
            args.prompt.write_text(prompt)
        server['env'].setdefault('WORKSPACE_ROOT',str(lab))
        clasify, classifiers = semantic_gates(server, env)
        metadata={'runner':'codex','model':args.model,'mcp':True,'timeout':args.timeout,
            'max_tool_calls':args.max_tool_calls,'budget_unit':'tool_calls','builtins_disabled':list(DISABLED_FEATURES),
            'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest(),
            'isolation':'explicit MCP only; fresh Octocode state; path-policy canary; host auth retained',
            'usage_scope':'Codex reported solver tokens; JEV provider tokens are separate'}
        (run/'runner.json').write_text(json.dumps(metadata,indent=1))
        try:
            receipt=probe(server,clasify,classifiers,env,lab,sentinel)
            (run/'preflight.json').write_text(json.dumps(receipt,indent=1))
            if not receipt['ok']: raise RuntimeError('; '.join(receipt['problems']))
        except Exception as exc:
            reason='preflight_failed'; errors.append(str(exc))
            (run/'transcript.jsonl').write_text(json.dumps({'type':'bench_end','_t':time.time(),'start':started,'exit':2,'reason':reason,'errors':errors})+'\n')
            return 2
        cmd=command(args.model,lab,server)
        messages=queue.Queue()
        with (run/'transcript.jsonl').open('w') as out,(run/'stderr.log').open('w') as err:
            solver_started=time.time()
            proc=subprocess.Popen(cmd,cwd=lab,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,start_new_session=True)
            proc.stdin.write(prompt); proc.stdin.close()
            def read():
                for line in proc.stdout: messages.put(line)
                messages.put(None)
            reader=threading.Thread(target=read,daemon=True); reader.start()
            try:
                while True:
                    if time.time()-started>args.timeout: reason='timeout'; break
                    try: line=messages.get(timeout=0.2)
                    except queue.Empty:
                        if proc.poll() is not None: break
                        continue
                    if line is None: break
                    try: event=json.loads(line)
                    except ValueError: err.write(line); continue
                    event['_t']=time.time(); out.write(json.dumps(event)+'\n'); out.flush()
                    item=event.get('item',{})
                    if event.get('type')=='item.started' and item.get('type')=='mcp_tool_call':
                        calls+=1
                        if calls>args.max_tool_calls: reason='tool_call_cap'; break
                        if item.get('server')!='octocode': reason='unexpected_tool'; break
                    if item.get('type') in ('command_execution','file_change','web_search'):
                        reason='unexpected_tool'; break
            finally:
                # Stdout EOF can precede process exit. Avoid signaling a successful
                # worker while the host is finishing its own shutdown.
                if reason is None:
                    try: proc.wait(timeout=min(3, max(0, args.timeout-(time.time()-started))))
                    except subprocess.TimeoutExpired: reason='exit_timeout'
                terminate(proc); reader.join(timeout=3); proc.stdout.close()
            ended=time.time()
            end={'type':'bench_end','_t':ended,'start':started,'exit':proc.returncode,
                'setup_ms':round((solver_started-started)*1000),'solver_ms':round((ended-solver_started)*1000),
                'reason':reason,'tool_calls':calls,'errors':errors}
            out.write(json.dumps(end)+'\n')
        # Preserve non-secret runtime statistics, not auth/config/cache state.
        stats=home/'stats.json'
        if stats.exists(): (run/'octocode-stats.json').write_bytes(stats.read_bytes())
    return 0 if proc.returncode==0 and reason is None else 2
if __name__=='__main__':
    entered = time.time()
    try:
        raise SystemExit(main())
    except Exception as exc:
        if '--run' in sys.argv:
            run = Path(sys.argv[sys.argv.index('--run')+1])
            run.mkdir(parents=True,exist_ok=True)
            trace_path = run/'runner-error.log'
            trace_path.write_text(traceback.format_exc())
            now = time.time()
            with (run/'transcript.jsonl').open('a') as output:
                output.write(json.dumps({'type':'bench_end','start':entered,'_t':now,'exit':2,
                    'reason':'runner_setup_failed','errors':[type(exc).__name__+': '+str(exc)],
                    'traceback_path':str(trace_path)})+'\n')
        print('Codex runner setup failed: '+str(exc),file=sys.stderr)
        raise SystemExit(2)
