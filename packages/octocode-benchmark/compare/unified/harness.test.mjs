import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { spawn, execFileSync } from 'node:child_process';
import { parseStream, tokenAccounting, toolCounts, runClaude, configurationHashes, sha256, recordedProbe, failedProbeDetails, pool, freshCwd, REPO_ROOT, resumedAccounting } from './lib.mjs';
import { getConfigFilePath, getProjectConfigFilePath, propagateOctocodeEnv } from '@octocodeai/config';
import { parseVerdict } from './verdict.mjs';
import { solverBoundary, sandboxPolicy, nativeSandboxPolicy } from './isolation.mjs';


test('canonical global and workspace configuration bytes invalidate the frozen input key', () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-config-'));
  const home = path.join(fixture, 'home'), cwd = path.join(fixture, 'workspace');
  const globalConfig = getConfigFilePath(home), workspaceConfig = getProjectConfigFilePath(cwd);
  const globalEnv = path.join(path.dirname(globalConfig), '.env');
  const workspaceEnv = path.join(path.dirname(workspaceConfig), '.env');
  const files = [globalConfig, workspaceConfig, globalEnv, workspaceEnv];
  try {
    for (const file of files) {
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, file.endsWith('.env') ? 'OCTOCODE_BENCH_FIXTURE_VALUE=global\n' : '{}\n');
    }
    fs.writeFileSync(workspaceEnv, 'OCTOCODE_BENCH_FIXTURE_VALUE=workspace\n');
    const env = {};
    propagateOctocodeEnv({ env, home, cwd, trusted: true });
    assert.equal(env.OCTOCODE_BENCH_FIXTURE_VALUE, 'workspace', 'canonical workspace dotenv overrides global');
    fs.unlinkSync(workspaceEnv);
    const globalOnly = {};
    propagateOctocodeEnv({ env: globalOnly, home, cwd, trusted: true });
    assert.equal(globalOnly.OCTOCODE_BENCH_FIXTURE_VALUE, 'global', 'canonical global dotenv is loaded');
    fs.writeFileSync(workspaceEnv, 'OCTOCODE_BENCH_FIXTURE_VALUE=workspace\n');
    const key = () => sha256(JSON.stringify(configurationHashes(cwd, home)));
    const original = key();
    for (const file of files) {
      const bytes = fs.readFileSync(file);
      // Even an unused value/comment is a changed experimental input.
      fs.appendFileSync(file, file.endsWith('.env') ? '# changed fixture input\n' : ' \n');
      assert.notEqual(key(), original, `canonical file mutation must invalidate key: ${path.basename(file)}`);
      fs.writeFileSync(file, bytes);
      assert.equal(key(), original);
    }
    fs.writeFileSync(path.join(cwd, '.env'), 'UNUSED_ROOT_ENV=changed\n');
    assert.equal(key(), original, 'root .env is not a canonical runtime config input');
    fs.unlinkSync(workspaceEnv);
    assert.notEqual(key(), original, 'removing a canonical input must invalidate key');
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});


test('failed probe receipts and pooled gate details retain identity and redact evaluator secrets', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-probe-errors-'));
  const secret = 'synthetic-private-credential';
  try {
    const jobs = ['octocode-overhead', 'octocode-isolation', 'rg-gh-overhead', 'rg-gh-isolation'].map((id, i) => () => recordedProbe({ recordPath: path.join(dir, `${id}.json`), worker: id.startsWith('octocode') ? 'octocode' : 'rg-gh', probe: id.endsWith('overhead') ? 'overhead' : 'isolation', secrets: [secret] }, async () => {
      if (i !== 2) throw Object.assign(new Error(`listen EINVAL for session; Bearer ${secret}; API_KEY=another-sensitive-value`), { code: 'EINVAL' });
      return { worker: 'rg-gh', probe: 'overhead', status: 'ok', costVerified: true, isolation: { ok: true } };
    }));
    const records = await pool(jobs, 2);
    const failures = failedProbeDetails(records);
    assert.equal(failures.length, 3);
    assert.deepEqual(failures.map(f => `${f.worker}/${f.probe}`), ['octocode/overhead', 'octocode/isolation', 'rg-gh/isolation']);
    for (const failure of failures) {
      assert.equal(failure.error.code, 'EINVAL');
      assert.match(failure.error.message, /listen EINVAL/);
      assert.equal(failure.costVerified, false);
    }
    for (const file of fs.readdirSync(dir)) {
      const content = fs.readFileSync(path.join(dir, file), 'utf8');
      assert.doesNotMatch(content, /synthetic-private-credential|another-sensitive-value/);
      assert.ok(JSON.parse(content).worker);
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
test('long temporary roots and session labels still permit the actual macOS gateway socket', { skip: process.platform !== 'darwin', timeout: 10000 }, async () => {
  const original = process.env.TMPDIR;
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-long-root-'));
  const longRoot = path.join(root, 'x'.repeat(100)); fs.mkdirSync(longRoot);
  let cwd, boundary;
  try {
    process.env.TMPDIR = longRoot;
    cwd = freshCwd('probe-isolation-' + 'long'.repeat(200));
    assert.ok(Buffer.byteLength(path.join(fs.realpathSync(cwd), 'github.sock')) < 104);
    boundary = await solverBoundary({ cwd, corpus: [], repoRoot: REPO_ROOT, oauthToken: 'synthetic' });
    assert.ok(fs.existsSync(boundary.githubSocket));
  } finally {
    if (original === undefined) delete process.env.TMPDIR; else process.env.TMPDIR = original;
    await boundary?.close(); if (cwd) fs.rmSync(cwd, { recursive: true, force: true }); fs.rmSync(root, { recursive: true, force: true });
  }
});
test('actual socket listen failure closes the already spawned native upstream', { skip: process.platform !== 'darwin', timeout: 10000 }, async () => {
  const cwd = freshCwd('listen-failure');
  const children = () => execFileSync('/bin/ps', ['-axo', 'pid=,ppid=,command='], { encoding: 'utf8' }).split('\n').map(line => line.match(/^\s*(\d+)\s+(\d+)\s+(.+)$/)).filter(row => row && Number(row[2]) === process.pid && !row[3].startsWith('/bin/ps ')).map(row => Number(row[1]));
  const before = children();
  try {
    fs.writeFileSync(path.join(cwd, 'github.sock'), 'occupied socket path');
    await assert.rejects(solverBoundary({ cwd, corpus: [path.join(REPO_ROOT, 'octocode-local-testing/fixtures')], repoRoot: REPO_ROOT, mcp: true, statsHome: path.join(cwd, 'native-home'), oauthToken: 'synthetic' }), error => error.code === 'EADDRINUSE');
    const remaining = children().filter(pid => !before.includes(pid));
    assert.deepEqual(remaining, [], 'no evaluator-native process survives failed startup');
  } finally { fs.rmSync(cwd, { recursive: true, force: true }); }
});

const usage = { input_tokens: 2, cache_creation_input_tokens: 500, cache_read_input_tokens: 1000, output_tokens: 6 };
const stream = events => events.map(e => JSON.stringify(e)).join('\n');
test('final usage wins over provisional message usage; TTL and discrepancies retained', () => {
  const parsed = parseStream(stream([
    { type: 'assistant', message: { id: 'm', usage, content: [] } },
    { type: 'result', subtype: 'success', is_error: false, usage: { ...usage, output_tokens: 1200, cache_creation: { ephemeral_1h_input_tokens: 500, ephemeral_5m_input_tokens: 0 } }, modelUsage: { model: { inputTokens: 2, cacheCreationInputTokens: 500, cacheReadInputTokens: 1000, outputTokens: 1200 } } },
  ]));
  const totals = tokenAccounting(parsed);
  assert.equal(totals.output_tokens, 1200);
  assert.equal(totals.total_tokens, 2702);
  assert.equal(totals.usage_gaps.output_tokens, -1194);
  assert.equal(totals.cache_creation.ephemeral_1h_input_tokens, 500);
  assert.equal(totals.verified, true);
  assert.equal(totals.weighted_tokens, null);
  assert.equal(totals.overhead_research_estimated, true);
});

test('resumed current usage reconciles with cumulative model/cost delta without double counting', () => {
  const baseUsage = { input_tokens: 6, cache_creation_input_tokens: 18791, cache_read_input_tokens: 47835, output_tokens: 2992 };
  const currentUsage = { input_tokens: 2, cache_creation_input_tokens: 21809, cache_read_input_tokens: 0, output_tokens: 536, cache_creation: { ephemeral_1h_input_tokens: 21809, ephemeral_5m_input_tokens: 0 } };
  const parsed = (usage, model, cost) => parseStream(stream([{ type: 'result', session_id: 'same-session', subtype: 'success', is_error: false, usage, modelUsage: { model }, total_cost_usd: cost }]));
  const baseline = parsed(baseUsage, { inputTokens: 6, cacheCreationInputTokens: 18791, cacheReadInputTokens: 47835, outputTokens: 2992, costUSD: 0.114663 }, 0.114663);
  const resumed = parsed(currentUsage, { inputTokens: 8, cacheCreationInputTokens: 40600, cacheReadInputTokens: 47835, outputTokens: 3528, costUSD: 0.207263 }, 0.207263);
  assert.equal(tokenAccounting(resumed).verified, false, 'different scopes must not be silently accepted');
  const accounting = resumedAccounting(baseline, resumed);
  assert.equal(accounting.verified, true);
  assert.equal(accounting.tokens.total_tokens, 22347);
  assert.equal(accounting.tokens.cache_creation.ephemeral_1h_input_tokens, 21809);
  assert.ok(Math.abs(accounting.cost_usd - 0.0926) < 1e-9);
  assert.equal(accounting.reportedCumulative.cost_usd, 0.207263);
  assert.equal(accounting.baseline.cost_usd, 0.114663);
  assert.equal(accounting.modelUsageDelta.model.outputTokens, 536);
  assert.equal(resumed.total_cost_usd, 0.207263, 'raw parsed receipt remains unchanged');
  const variants = [
    { ...resumed, sessionId: 'other-session' },
    { ...resumed, total_cost_usd: 0.01 },
    { ...resumed, modelUsage: { model: { ...resumed.modelUsage.model, outputTokens: 100 } } },
    { ...resumed, modelUsage: { model: { ...resumed.modelUsage.model, outputTokens: 3529 } } },
    { ...resumed, modelUsage: { model: { ...resumed.modelUsage.model, costUSD: 0.21 } } },
    { ...resumed, modelUsage: null },
  ];
  for (const invalid of variants) {
    const result = resumedAccounting(baseline, invalid);
    assert.equal(result.verified, false);
    assert.equal(result.tokens.verified, false);
    assert.equal(result.costVerified, false);
    assert.ok(result.reasons.length > 0);
  }
});

test('missing final usage and disagreeing model accounting cannot certify metrics', () => {
  assert.equal(tokenAccounting(parseStream(stream([{ type: 'assistant', message: { id: 'm', usage, content: [] } }]))).verified, false);
  const parsed = parseStream(stream([{ type: 'result', usage, modelUsage: { m: { inputTokens: 99 } } }]));
  assert.equal(tokenAccounting(parsed).verified, false);
});
test('stable tool IDs deduplicate calls/results and structured mixed-row failures survive', () => {
  const call = { type: 'assistant', message: { id: 'm', usage, content: [{ type: 'tool_use', id: 't', name: 'Bash', input: { command: 'cd /tmp && rg needle' } }] } };
  const result = { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't', content: [{ type: 'text', text: JSON.stringify({ results: [{ status: 'ok' }, { status: 'error', data: { errorCode: 'notFound' } }] }) }] }] } };
  const parsed = parseStream(stream([call, call, result, result]));
  assert.equal(parsed.toolCalls.length, 1);
  assert.equal(parsed.rowErrors.length, 1);
  assert.deepEqual(toolCounts(parsed.toolCalls), { 'Bash:invocation': 1 });
});
test('malformed grader types fail closed', () => {
  const row = { correctness: null, completeness: true, evidence: 0, quality: 1 };
  assert.equal(parseVerdict('```json\n' + JSON.stringify({ X: row, Y: row, preferred: 'tie' }) + '\n```'), null);
});

const childRun = (command, args, options) => new Promise((resolve, reject) => {
  const child = spawn(command, args, options); let stdout = '', stderr = '';
  child.stdout.on('data', bytes => stdout += bytes); child.stderr.on('data', bytes => stderr += bytes);
  child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal, stdout, stderr }));
});
test('unsupported isolation is explicit', { skip: process.platform === 'darwin' }, () => {
  assert.throws(() => sandboxPolicy({ cwd: os.tmpdir() }), /fails closed/);
});
test('actual sandbox: evaluator/corpus/network denial, real gh GET, implicit POST rejection, and no secret exposure', { skip: process.platform !== 'darwin', timeout: 30000 }, async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-test-'));
  const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'evaluator-unrelated-name-'));
  const corpus = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-corpus-'));
  let boundary; let forwarded = 0;
  try {
    fs.mkdirSync(path.join(outside, '.codex')); fs.writeFileSync(path.join(outside, '.codex', 'arbitrary-history'), 'private');
    fs.writeFileSync(path.join(outside, 'arbitrary-key'), 'private');
    fs.writeFileSync(path.join(corpus, 'source.txt'), 'public-source');
    const nativeReadOnly = await childRun('/usr/bin/sandbox-exec', ['-p', nativeSandboxPolicy([corpus]), process.execPath, '-e', `const fs=require('fs');if(fs.readFileSync(${JSON.stringify(path.join(corpus, 'source.txt'))},'utf8')!=='public-source')process.exit(5);try{fs.writeFileSync(${JSON.stringify(path.join(corpus, 'source.txt'))},'wrong');process.exit(6)}catch{}`], { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    assert.equal(nativeReadOnly.code, 0, nativeReadOnly.stderr);
    boundary = await solverBoundary({ cwd, corpus: [corpus], repoRoot: process.cwd(), oauthToken: 'synthetic', githubToken: 'evaluator-private', githubFetch: async (url, options) => { forwarded++; assert.equal(options.method, 'GET'); assert.equal(options.headers.Authorization, 'Bearer evaluator-private'); return new Response(JSON.stringify({ path: url.pathname }), { status: 200 }); } });
    assert.equal(boundary.env.GH_TOKEN, 'local-readonly-gateway');
    const get = await childRun('/usr/bin/sandbox-exec', ['-f', boundary.sandboxProfile, 'gh', 'api', 'user'], { cwd, env: boundary.env });
    assert.equal(get.code, 0, get.stderr); assert.equal(JSON.parse(get.stdout).path, '/user');
    const post = await childRun('/usr/bin/sandbox-exec', ['-f', boundary.sandboxProfile, 'gh', 'api', 'repos/example/test/issues', '-f', 'title=forbidden'], { cwd, env: boundary.env });
    assert.equal(post.code, 1); assert.match(post.stderr, /405/); assert.equal(forwarded, 1);
    const privateDir = path.join(cwd, 'octocode-benchmark'); fs.mkdirSync(privateDir); const privateFile = path.join(privateDir, 'prior-answer'); fs.writeFileSync(privateFile, 'private');
    const denied = await childRun('/usr/bin/sandbox-exec', ['-f', boundary.sandboxProfile, process.execPath, '-e', `try{require('fs').readFileSync(${JSON.stringify(privateFile)});process.exit(9)}catch{process.exit(0)}`], { cwd, env: boundary.env });
    assert.equal(denied.code, 0);
    const encoded = Buffer.from(path.join(outside, 'arbitrary-key')).toString('base64');
    const inherited = await childRun('/usr/bin/sandbox-exec', ['-f', boundary.sandboxProfile, '/bin/bash', '-c', `${process.execPath} -e 'const fs=require("fs");for(const p of [Buffer.from("${encoded}","base64").toString(),${JSON.stringify(path.join(outside, '.codex', 'arbitrary-history'))}]){try{fs.readFileSync(p);process.exit(8)}catch{}};if(fs.readFileSync(${JSON.stringify(path.join(corpus, 'source.txt'))},"utf8")!=="public-source")process.exit(7);try{fs.writeFileSync(${JSON.stringify(path.join(corpus, 'source.txt'))},"wrong");process.exit(6)}catch{}'`], { cwd, env: boundary.env });
    assert.equal(inherited.code, 0, inherited.stderr);
    const connect = await new Promise((resolve, reject) => { const req = http.request({ hostname: '127.0.0.1', port: boundary.proxyPort, method: 'CONNECT', path: 'api.github.com:443' }); req.on('connect', res => { req.destroy(); resolve(res.statusCode); }); req.on('response', res => { res.resume(); resolve(res.statusCode); }); req.on('error', reject); req.end(); });
    assert.equal(connect, 403);
  } finally { await boundary?.close(); for (const dir of [cwd, outside, corpus]) fs.rmSync(dir, { recursive: true, force: true }); }
});
test('spawn failure rejects and does not hang', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-spawn-test-'));
  try { await assert.rejects(runClaude({ args: [], cwd, timeoutMs: 1000, streamPath: path.join(cwd, 'stream'), env: { PATH: '/does-not-exist' } }), /ENOENT/); }
  finally { fs.rmSync(cwd, { recursive: true, force: true }); }
});
test('deadline kills inherited descendants even when the parent closes first', { skip: process.platform === 'win32', timeout: 10000 }, async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-deadline-test-'));
  const executable = path.join(cwd, 'claude');
  try {
    fs.writeFileSync(executable, `#!/bin/sh\n${process.execPath} -e 'process.on("SIGTERM",()=>{});setInterval(()=>{},1000)' >/dev/null 2>&1 &\necho $! > descendant.pid\nwait\n`);
    fs.chmodSync(executable, 0o755);
    const result = await runClaude({ args: [], cwd, timeoutMs: 1500, streamPath: path.join(cwd, 'stream'), env: { ...process.env, PATH: `${cwd}:${process.env.PATH}` } });
    assert.equal(result.timedOut, true);
    const pid = Number(fs.readFileSync(path.join(cwd, 'descendant.pid'), 'utf8'));
    await new Promise(resolve => setTimeout(resolve, 100));
    assert.throws(() => process.kill(pid, 0), /ESRCH/);
  } finally { fs.rmSync(cwd, { recursive: true, force: true }); }
});
