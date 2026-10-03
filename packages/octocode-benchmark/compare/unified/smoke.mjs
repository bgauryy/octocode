#!/usr/bin/env node
// Native bridge probe is free. --live-model explicitly runs one tiny billed model
// request; it never loads benchmark questions, references, answers, or graders.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { solverBoundary, evaluatorCredentials } from './isolation.mjs';
import { parseArgs, runClaude, parseStream, tokenAccounting, REPO_ROOT, loadWorkers, workerFlags, commonFlags, isolationCheck } from './lib.mjs';

const args = parseArgs(process.argv.slice(2));
const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'ocbench-live-smoke-'));
const evaluatorDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-benchmark-smoke-'));
const statsHome = path.join(evaluatorDir, '.octocode');
let boundary;
try {
  const corpus = [path.join(REPO_ROOT, 'octocode-local-testing/fixtures')];
  const useMcp = args.worker !== 'rg-gh';
  boundary = await solverBoundary({ cwd, corpus, repoRoot: REPO_ROOT, mcp: useMcp, statsHome, ...evaluatorCredentials() });
  if (useMcp) {
  const child = spawn('/usr/bin/sandbox-exec', ['-f', boundary.sandboxProfile, process.execPath, boundary.bridge], { cwd, env: boundary.env, stdio: ['pipe', 'pipe', 'pipe'] });
  let buffer = '', stderr = '', id = 0;
  const pending = new Map();
  const rejectAll = error => { for (const entry of pending.values()) entry.reject(error); pending.clear(); };
  child.on('error', rejectAll); child.on('close', code => { if (code) rejectAll(new Error(`bridge exited ${code}: ${stderr.slice(-1000)}`)); });
  child.stderr.on('data', c => stderr += c);
  child.stdout.on('data', c => {
    buffer += c; let at;
    while ((at = buffer.indexOf('\n')) >= 0) { const line = buffer.slice(0, at); buffer = buffer.slice(at + 1); try { const msg = JSON.parse(line); pending.get(msg.id)?.resolve(msg); } catch { rejectAll(new Error('invalid bridge frame')); } }
  });
  const rpc = async (method, params) => {
    const n = ++id;
    let timer;
    try { return await new Promise((resolve, reject) => { timer = setTimeout(() => reject(new Error(`smoke ${method} deadline exceeded`)), 30000); pending.set(n, { resolve, reject }); child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: n, method, params }) + '\n'); }); }
    finally { clearTimeout(timer); pending.delete(n); }
  };
  try {
    const init = await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'isolated-native-smoke', version: '1' } });
    if (init.error) throw new Error(init.error.message);
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    const catalog = await rpc('tools/list', {});
    if (!catalog.result?.tools?.some(t => t.name === 'structureSearch')) throw new Error('native MCP catalog missing');
    const result = await rpc('tools/call', { name: 'structureSearch', arguments: { queries: [{ path: corpus[0], operation: 'files', limit: 2, goal: 'Verify isolated native MCP', reasoning: 'Smoke probe' }] } });
    if (result.error || result.result?.isError || !result.result?.structuredContent) throw new Error('native MCP tool probe failed');
    const mixed = await rpc('tools/call', { name: 'structureSearch', arguments: { queries: [{ path: corpus[0], operation: 'files', limit: 1, goal: 'Verify row accounting', reasoning: 'Smoke probe' }, { path: path.join(corpus[0], '__nonexistent_smoke__'), operation: 'files', goal: 'Verify row accounting', reasoning: 'Intentional missing path' }] } });
    if (!mixed.result?.structuredContent || !boundary.nativeCalls().some(c => c.tool === 'structureSearch' && c.rowErrors.length > 0)) throw new Error('native mixed-row error telemetry failed');
    console.log(JSON.stringify({ nativeBridge: 'passed', tools: catalog.result.tools.length, structured: true, nativeRowErrorsVerified: true }));
  } finally { child.kill(); }
  }
  if (args['live-model'] || args['live-tools']) {
    if (!String(args.model ?? '').startsWith('claude-')) throw new Error('concrete --model required for live smoke');
    const worker = args['live-tools'] ? structuredClone(loadWorkers(String(args.worker ?? 'octocode'))[0]) : null;
    if (worker && !['octocode', 'rg-gh'].includes(worker.id)) throw new Error('smoke worker must be octocode or rg-gh');
    const flags = ['--strict-mcp-config', '--tools', ''];
    let prompt = 'Reply with exactly OK. Do not use tools.';
    if (worker) {
      const doc = path.join(cwd, 'WORKER.md'); fs.copyFileSync(worker.docPath, doc);
      if (worker.id === 'octocode') worker.profile.mcpServers = { octocode: { command: process.execPath, args: [boundary.bridge] } };
      const config = path.join(cwd, 'mcp.json'); fs.writeFileSync(config, JSON.stringify({ mcpServers: worker.profile.mcpServers ?? {} }));
      flags.splice(0, flags.length, '--append-system-prompt-file', doc, ...workerFlags(worker, corpus, config));
      prompt = worker.id === 'octocode' ? `Call structureSearch once with operation files, path ${corpus[0]}, limit 2. Then reply with exactly OK.` : `Use Bash to run gh api -X GET repos/microsoft/TypeScript --jq .full_name and rg --files ${corpus[0]} | head -n 2. Then reply with exactly OK.`;
    }
    const res = await runClaude({ args: ['-p', prompt, ...commonFlags({ model: args.model, maxTurns: worker ? 4 : 2 }), ...flags], cwd, timeoutMs: Number(args['timeout-ms'] ?? 120000), streamPath: path.join(cwd, 'model-stream.jsonl'), env: boundary.env, sandboxProfile: boundary.sandboxProfile });
    const m = parseStream(res.stream), totals = tokenAccounting(m);
    if (res.exitCode !== 0 || res.timedOut || m.isError || m.resultSubtype !== 'success' || !totals.verified || m.answer.trim() !== 'OK') throw new Error(`isolated model smoke failed: ${res.exitCode}/${m.resultSubtype} ${res.stderr.slice(-1500)} traffic=${JSON.stringify(boundary.traffic())} result=${m.answer.slice(-1000)}`);
    if (worker && (!isolationCheck(worker.profile, m).ok || !m.toolCalls.length || m.toolErrors.length || m.rowErrors.length || (worker.id === 'rg-gh' && !boundary.traffic().githubGetRequests))) throw new Error('actual worker tool integration failed');
    console.log(JSON.stringify({ model: m.actualModels, worker: worker?.id, modelConnection: 'passed', totalTokens: totals.total_tokens, costUsd: m.total_cost_usd, tools: m.toolCalls.map(c => c.name), traffic: boundary.traffic() }));
  }
} finally { await boundary?.close(); fs.rmSync(cwd, { recursive: true, force: true }); fs.rmSync(evaluatorDir, { recursive: true, force: true }); }
