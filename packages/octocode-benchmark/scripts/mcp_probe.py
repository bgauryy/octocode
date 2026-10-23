#!/usr/bin/env python3
"""Bounded MCP receipts: actual catalog gates and local path-policy denial."""
from __future__ import annotations
import argparse, hashlib, json, os, queue, signal, subprocess, threading, time
from pathlib import Path

class McpClient:
    def __init__(self, server, env=None, cwd=None):
        child = dict(env if env is not None else os.environ)
        child.update(server.get('env', {}))
        self.proc = subprocess.Popen([server['command'], *server.get('args', [])], env=child, cwd=cwd,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, start_new_session=True)
        self.messages = queue.Queue()
        self.counter = 0
        def read():
            for line in self.proc.stdout:
                try: self.messages.put(json.loads(line))
                except ValueError: pass
            self.messages.put(None)
        threading.Thread(target=read, daemon=True).start()
    def call(self, method, params=None, timeout=30):
        self.counter += 1
        ident = self.counter
        self.proc.stdin.write(json.dumps({'jsonrpc':'2.0','id':ident,'method':method,'params':params or {}})+'\n')
        self.proc.stdin.flush()
        deadline = time.monotonic()+timeout
        while time.monotonic()<deadline:
            try: msg=self.messages.get(timeout=max(0.001, deadline-time.monotonic()))
            except queue.Empty: break
            if msg is None: raise RuntimeError('MCP exited before response')
            if msg.get('id')==ident:
                if 'error' in msg: raise RuntimeError(json.dumps(msg['error']))
                return msg['result']
        raise TimeoutError('MCP request deadline: '+method)
    def close(self):
        if self.proc.poll() is None:
            os.killpg(self.proc.pid, signal.SIGTERM)
            try: self.proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL); self.proc.wait()
        self.proc.stdin.close(); self.proc.stdout.close()


def has_classifiers(value):
    if isinstance(value, dict):
        return 'classifiers' in value.get('properties', {}) or any(has_classifiers(v) for v in value.values())
    return isinstance(value,list) and any(has_classifiers(v) for v in value)

CLASSIFIER_READERS = {'localSearch','localFetch','ghGetFileContent','ghSearchCode'}
QUESTIONS = {'evidence':{'type':'noul','instructions':'Does this contain the requested evidence?'}}


def probe(server, expected_clasify, expected_classifiers, env=None, cwd=None, forbidden=None):
    client=McpClient(server,env,cwd)
    try:
        init=client.call('initialize',{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'octocode-benchmark-preflight','version':'2'}})
        client.proc.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); client.proc.stdin.flush()
        catalog=client.call('tools/list')
        tools={t['name']:t for t in catalog['tools']}
        classifiers=sorted(n for n,t in tools.items() if has_classifiers(t.get('inputSchema',{})))
        problems=[]
        if ('clasify' in tools)!=expected_clasify: problems.append('clasify gate differs from requested arm')
        readers={'localSearch','localFetch','structureSearch','astSearch','lspSearch','ghSearchRepo','ghSearchCode','ghStructure','ghGetFileContent','ghSearchHistory','ghGetHistoryItem','artifactSearch'}
        selected = server.get('env', {}).get('TOOLS_TO_RUN', '')
        if selected:
            allowed = set(selected.split(','))
            readers = readers.intersection(allowed)
            if set(tools).difference(allowed): problems.append('unexpected tools outside explicit allowlist')
        missing = readers.difference(tools)
        if missing: problems.append('required read tools missing: ' + ', '.join(sorted(missing)))
        expected=CLASSIFIER_READERS.intersection(readers) if expected_classifiers else set()
        if set(classifiers)!=expected: problems.append('classifiers schema gate differs from eligible read tools')
        denials=[]
        if forbidden:
            calls=[('localFetch',{'queries':[{'path':str(forbidden),'ranges':['1-5']}]})]
            if expected_clasify and "clasify" in tools:
                calls.append(('clasify',{'queries':[{'source':{'tool':'localFetch','query':{'path':str(forbidden),'ranges':['1-5']}},'questions':QUESTIONS}]}))
            for tool,args in calls:
                reply=client.call('tools/call',{'name':tool,'arguments':args})
                text=json.dumps(reply)
                denied=any(mark in text for mark in ['outsideAllowedRoots','outside allowed roots','Outside allowed roots','not within allowed'])
                denials.append({'tool':tool,'denied':denied,'response':reply})
                if not denied: problems.append(tool+' did not demonstrate forbidden-root denial')
            if expected_clasify and 'clasify' in tools and selected and 'ghGetFileContent' not in tools:
                # A local allowlist leaves the GitHub reader off; its source is refused.
                reply=client.call('tools/call', {'name':'clasify','arguments':{'queries':[{
                    'source':{'tool':'ghGetFileContent','query':{'owner':'octocode-benchmark-disabled-probe',
                        'repo':'probe','path':'README.md','fullContent':True}},
                    'questions':QUESTIONS}]}})
                body=json.dumps(reply).lower()
                denied='ghgetfilecontent' in body and ('not available' in body or 'unavailable' in body or 'not enabled' in body or 'is disabled' in body)
                denials.append({'tool':'clasify:disabledRemote','denied':denied,'response':reply})
                if not denied: problems.append('clasify did not demonstrate disabled remote-tool denial')
        encoded=json.dumps(catalog,ensure_ascii=False,sort_keys=True)
        return {'ok':not problems,'problems':problems,'toolNames':sorted(tools),'classifiersTools':classifiers,
            'catalogSha256':hashlib.sha256(encoded.encode()).hexdigest(),'catalogChars':len(encoded),
            'initialize':init,'catalog':catalog,'denials':denials}
    finally: client.close()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--config',type=Path,required=True)
    ap.add_argument('--clasify',action='store_true'); ap.add_argument('--classifiers',action='store_true')
    ap.add_argument('--forbidden',type=Path); ap.add_argument('--out',type=Path)
    args=ap.parse_args()
    server=json.loads(args.config.read_text())['mcpServers']['octocode']
    receipt=probe(server,args.clasify,args.classifiers,forbidden=args.forbidden)
    if args.out: args.out.write_text(json.dumps(receipt,indent=1))
    print(json.dumps({k:receipt[k] for k in ['ok','problems','toolNames','classifiersTools','catalogChars','catalogSha256']}))
    return 0 if receipt['ok'] else 2
if __name__=='__main__': raise SystemExit(main())
