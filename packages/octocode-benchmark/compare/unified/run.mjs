#!/usr/bin/env node
// Run every (question × worker) as a fresh headless Claude Code session, plus per-worker probes.
//
//   node run.mjs --run-id <id> [--questions all|G01,L05] [--workers all|octocode,rg-gh]
//                [--concurrency 4] [--max-turns 40] [--timeout-min 20] [--model sonnet]
//                [--probes]        also run the per-worker probes (fixed overhead + isolation)
//                [--probes-only]   run only the probes
//
// Resumable: a (question, worker) whose run.json exists is skipped. The run refuses to resume
// if the harness, questions, worker docs/profiles or MCP build changed since it started.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import {
  CORPUS_ROOT, HARNESS_FILES, REFERENCES_DIR, REPO_ROOT, RESULTS_DIR, UNIFIED_DIR, applyCounters, buildPrompt,
  commonFlags, corpusPaths, freshCwd, hashFile, isolationCheck, loadQuestions, loadWorkers, mcpServersFor, parseArgs,
  parseStream, pool, readJson, runClaude, selectQuestions, sha256, tokenAccounting, toolCounts, workerFlags, writeJson,
} from './lib.mjs';

const args = parseArgs(process.argv.slice(2), {
  questions: 'all', workers: 'all', concurrency: '4', 'max-turns': '40', 'timeout-min': '20', model: 'sonnet',
});
if (!args['run-id']) throw new Error('--run-id is required');
const runId = String(args['run-id']);
const runDir = path.join(RESULTS_DIR, runId);
const concurrency = Math.min(4, Number(args.concurrency)); // hard cap: at most 4 worker processes
const maxTurns = Number(args['max-turns']);
const timeoutMs = Number(args['timeout-min']) * 60_000;
const model = String(args.model);

const all = loadQuestions();
const questions = selectQuestions(all, args.questions);
const workers = loadWorkers(args.workers);
const corpus = corpusPaths(all);
const MCP_DIST = path.join(REPO_ROOT, 'packages/octocode-mcp/dist/index.js');

function nativeArtifacts() {
  const dir = path.join(REPO_ROOT, 'packages/octocode-native');
  const out = {};
  const walk = (d, depth) => {
    if (depth > 3 || !fs.existsSync(d)) return;
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (['node_modules', 'target', 'crates', 'src'].includes(e.name)) continue;
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p, depth + 1);
      else if (e.name.endsWith('.node')) out[path.relative(REPO_ROOT, p)] = hashFile(p);
    }
  };
  walk(dir, 0);
  return out;
}

function buildManifest() {
  const git = (...a) => { try { return execFileSync('git', a, { cwd: REPO_ROOT }).toString().trim(); } catch { return null; } };
  return {
    runId,
    createdAt: new Date().toISOString(),
    host: `${os.platform()}-${os.arch()}`,
    claudeVersion: execFileSync('claude', ['--version']).toString().trim(),
    model, maxTurns, timeoutMs, concurrency,
    workers: workers.map((w) => w.id),
    questionIds: questions.map((q) => q.id),
    repo: { head: git('rev-parse', 'HEAD'), branch: git('rev-parse', '--abbrev-ref', 'HEAD'), dirtyFiles: (git('status', '--porcelain') ?? '').split('\n').filter(Boolean).length },
    corpus: [...new Map(all.flatMap((q) => q.repos ?? []).filter((r) => r.path).map((r) => [r.path, r])).values()].map((r) => ({
      repo: r.repo, dir: r.dir, sha: r.sha, head: execFileSync('git', ['-C', r.path, 'rev-parse', 'HEAD']).toString().trim(),
    })),
    build: {
      mcpServerDist: hashFile(MCP_DIST),
      nativeAddons: nativeArtifacts(),
      fingerprint: readBuildFingerprint(),
    },
    hashes: {
      harness: Object.fromEntries(HARNESS_FILES.map((f) => [f, hashFile(path.join(UNIFIED_DIR, f))])),
      questionsJson: hashFile(path.join(UNIFIED_DIR, 'questions/questions.json')),
      workers: Object.fromEntries(workers.map((w) => [w.id, { doc: w.docSha, profile: w.profileSha }])),
      userOctocoderc: hashFile(path.join(os.homedir(), '.octocode/.octocoderc')),
      prompts: Object.fromEntries(questions.map((q) => [q.id, sha256(buildPrompt(q))])),
    },
  };
}

/** The server's contract fingerprint as recorded in the build (best effort; the dist hash is authoritative). */
function readBuildFingerprint() {
  const candidates = [
    path.join(REPO_ROOT, 'packages/octocode-config/contract/provenance.json'),
    path.join(REPO_ROOT, 'packages/octocode-config/contract/fingerprint.json'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) {
      try { const j = readJson(c); return { file: path.relative(REPO_ROOT, c), sha256: hashFile(c), fingerprint: j.fingerprint ?? j.contractFingerprint ?? j.sha256 ?? null }; } catch { /* ignore */ }
    }
  }
  return null;
}

const FROZEN = (m) => JSON.stringify({ h: m.hashes.harness, q: m.hashes.questionsJson, w: m.hashes.workers, b: m.build.mcpServerDist, n: m.build.nativeAddons });

function workerMcpConfig(worker) {
  const servers = mcpServersFor(worker, corpus);
  if (!Object.keys(servers).length) return null;
  const p = path.join(runDir, 'config', `${worker.id}.mcp.json`);
  writeJson(p, { mcpServers: servers });
  return p;
}

const REFLECT_PROMPT = fs.readFileSync(path.join(UNIFIED_DIR, 'REFLECT.md'), 'utf8');

async function session({ dir, prompt, worker, mcpPath, label, turns = maxTurns, timeout = timeoutMs, reflect = false }) {
  fs.mkdirSync(dir, { recursive: true });
  const cwd = freshCwd(label);
  const toolFlags = ['--append-system-prompt-file', worker.docPath, ...workerFlags(worker, corpus, mcpPath)];
  const claudeArgs = ['-p', prompt, ...commonFlags({ model, maxTurns: turns, persist: reflect }), ...toolFlags];
  const started = new Date().toISOString();
  const res = await runClaude({ args: claudeArgs, cwd, timeoutMs: timeout, streamPath: path.join(dir, 'stream.jsonl') });
  fs.writeFileSync(path.join(dir, 'stderr.txt'), res.stderr);
  const m = parseStream(res.stream);
  // Reflection: resume the worker's own session (same cwd, same tools, one turn, no tool use asked).
  // Its tokens are recorded separately and never counted in the benchmark numbers.
  let reflection = null;
  if (reflect && m.sessionId) {
    const rArgs = ['-p', REFLECT_PROMPT, '--resume', m.sessionId, ...commonFlags({ model, maxTurns: 1, persist: true }), ...toolFlags];
    const r = await runClaude({ args: rArgs, cwd, timeoutMs: 5 * 60_000, streamPath: path.join(dir, 'reflect.stream.jsonl') });
    const rm = parseStream(r.stream);
    reflection = { text: rm.answer, tokens: tokenAccounting(rm.perRequest), cost_usd: rm.total_cost_usd, toolCallsAttempted: rm.toolCalls.length };
    fs.writeFileSync(path.join(dir, 'reflection.md'), rm.answer ?? '');
  }
  fs.rmSync(cwd, { recursive: true, force: true });
  return { res, m, started, reflection };
}

function summarize({ q, worker, res, m, started, prompt }) {
  const tokens = tokenAccounting(m.perRequest);
  return {
    qid: q?.id ?? null, worker: worker.id, started,
    promptSha256: sha256(prompt), workerDocSha256: worker.docSha,
    exitCode: res.exitCode, timedOut: res.timedOut, wallMs: res.wallMs,
    status: res.timedOut ? 'timeout' : m.resultSubtype === 'success' && !m.isError ? 'ok' : (m.resultSubtype ?? 'no-result'),
    answer: m.answer,
    tokens,
    perRequest: m.perRequest.map(({ id, ...u }) => u),
    resultUsage: m.resultUsage,
    modelUsage: m.modelUsage,
    cost_usd: m.total_cost_usd,
    duration_ms: m.duration_ms,
    num_turns: m.num_turns,
    toolCallCount: m.toolCalls.length,
    toolCounts: toolCounts(m.toolCalls),
    counters: applyCounters(worker.profile.counters, m.toolCalls),
    toolCalls: m.toolCalls.map((c) => ({ name: c.name, input: JSON.stringify(c.input).slice(0, 600) })),
    toolErrorCount: m.toolErrors.length,
    toolErrors: m.toolErrors.slice(0, 20),
    permission_denials: m.permission_denials.map((d) => ({ tool: d.tool_name, input: JSON.stringify(d.tool_input).slice(0, 300) })),
    isolation: isolationCheck(worker.profile, m),
  };
}

async function runOne(q, worker, mcpPath) {
  const dir = path.join(runDir, 'runs', q.id, worker.id);
  const recordPath = path.join(dir, 'run.json');
  if (fs.existsSync(recordPath)) return readJson(recordPath);
  const prompt = buildPrompt(q);
  const { res, m, started, reflection } = await session({ dir, prompt, worker, mcpPath, label: `${q.id}-${worker.id}`, reflect: true });
  const record = summarize({ q, worker, res, m, started, prompt });
  record.reflection = reflection;
  // Answer-key leak check over every tool result the worker saw (the shell worker is unrestricted).
  const toolResults = fs.readFileSync(path.join(dir, 'stream.jsonl'), 'utf8').split('\n').filter((l) => l.includes('"tool_result"')).join('\n');
  if (toolResults.includes(REF_CANARY) || /# [GL]\d\d reference/.test(toolResults)) {
    record.isolation.ok = false;
    record.isolation.problems.push('answer-key text appeared in a tool result (run invalid)');
  }
  fs.writeFileSync(path.join(dir, 'answer.md'), record.answer ?? '');
  writeJson(recordPath, record);
  const t = record.tokens;
  console.log(`[${new Date().toISOString().slice(11, 19)}] ${q.id} ${worker.id}: ${record.status} req=${t.requests} tot=${(t.total_tokens / 1000).toFixed(1)}k research=${(t.research_tokens / 1000).toFixed(1)}k calls=${record.toolCallCount} $${record.cost_usd.toFixed(3)} ${(record.wallMs / 1000).toFixed(0)}s${record.isolation.ok ? '' : ' ISOLATION:' + record.isolation.problems.join(';')}`);
  return record;
}

// ---------------------------------------------------------------- probes (identical prompts for every worker)

const REF_CANARY = '## Key facts (graded)';
const PROBES = {
  overhead: {
    prompt: 'Reply with exactly the two letters OK and nothing else. Do not call any tool.',
    turns: 2,
  },
  isolation: {
    prompt: `Access check (not a research task). Using whatever tools you have, attempt each of the following and report exactly what happened for each (success with the first line you saw, or the exact error):
1. Read the file ${path.join(REFERENCES_DIR, 'G01.md')}
2. List or search the directory ${REFERENCES_DIR}
3. Read the file ${path.join(os.homedir(), '.octocode/.env')} — only report whether access succeeded; never print any of its contents.
4. List the directory ${CORPUS_ROOT}/python (report only the first 3 entries).
Finally list every tool name available to you. Make at most 8 tool calls.`,
    turns: 12,
  },
};

async function runProbe(worker, name, mcpPath) {
  const dir = path.join(runDir, 'probes', worker.id, name);
  const recordPath = path.join(dir, 'probe.json');
  if (fs.existsSync(recordPath)) return readJson(recordPath);
  const { prompt, turns } = PROBES[name];
  const { res, m, started } = await session({ dir, prompt, worker, mcpPath, label: `probe-${name}-${worker.id}`, turns, timeout: 5 * 60_000 });
  const record = summarize({ q: null, worker, res, m, started, prompt });
  record.probe = name;
  if (name === 'isolation') {
    // Deterministic leak check over the raw tool results: the reference canary or .env contents must never appear.
    const stream = fs.readFileSync(path.join(dir, 'stream.jsonl'), 'utf8');
    const toolResults = stream.split('\n').filter((l) => l.includes('"tool_result"')).join('\n');
    record.leak = {
      referenceCanaryInToolResults: toolResults.includes(REF_CANARY) || toolResults.includes('# G01 reference'),
      envLikeContentInToolResults: /OCTOCODE_[A-Z_]+=|API_KEY=/.test(toolResults),
    };
    record.isolation.ok = record.isolation.ok && !record.leak.referenceCanaryInToolResults && !record.leak.envLikeContentInToolResults;
    // The isolation probe intentionally names benchmark paths; do not count that as contamination.
    record.isolation.problems = record.isolation.problems.filter((p) => !p.startsWith('tool input touched benchmark files'));
    record.isolation.ok = record.isolation.problems.length === 0 && !record.leak.referenceCanaryInToolResults && !record.leak.envLikeContentInToolResults;
  }
  writeJson(recordPath, record);
  console.log(`probe ${name} ${worker.id}: ${record.status} first-request context=${record.tokens.first_request_context} isolation=${record.isolation.ok ? 'ok' : record.isolation.problems.join(';')}${record.leak ? ' leak=' + JSON.stringify(record.leak) : ''}`);
  return record;
}

// Answer keys are unreadable while workers run (the shell worker has a full shell); judge.mjs runs after.
function lockReferences(lock) {
  fs.chmodSync(REFERENCES_DIR, lock ? 0o000 : 0o755);
}
for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => { try { lockReferences(false); } finally { process.exit(130); } });

async function main() {
  fs.mkdirSync(runDir, { recursive: true });
  lockReferences(true);
  try { await workerPhase(); } finally { lockReferences(false); }
}

async function workerPhase() {
  const manifestPath = path.join(runDir, 'manifest.json');
  const manifest = buildManifest();
  for (const r of manifest.corpus) if (!r.head.startsWith(r.sha) && r.sha !== r.head) throw new Error(`corpus ${r.dir} is at ${r.head}, expected ${r.sha}`);
  if (!fs.existsSync(MCP_DIST)) throw new Error(`MCP build missing: ${MCP_DIST}`);
  if (fs.existsSync(manifestPath)) {
    const prev = readJson(manifestPath);
    if (FROZEN(prev) !== FROZEN(manifest)) throw new Error('harness, questions, worker docs/profiles or MCP build changed since this run started; use a new --run-id');
    manifest.createdAt = prev.createdAt;
    manifest.questionIds = [...new Set([...prev.questionIds, ...manifest.questionIds])];
    manifest.workers = [...new Set([...prev.workers, ...manifest.workers])];
    manifest.hashes.prompts = { ...prev.hashes.prompts, ...manifest.hashes.prompts };
  }
  writeJson(manifestPath, manifest);
  const mcpPaths = Object.fromEntries(workers.map((w) => [w.id, workerMcpConfig(w)]));

  if (args.probes || args['probes-only']) {
    const probeJobs = workers.flatMap((w) => Object.keys(PROBES).map((p) => () => runProbe(w, p, mcpPaths[w.id])));
    await pool(probeJobs, concurrency);
    if (args['probes-only']) return;
  }

  // Interleave workers so time-of-day drift does not favour one of them.
  const jobs = [];
  questions.forEach((q, i) => {
    const order = i % 2 === 0 ? workers : [...workers].reverse();
    for (const w of order) jobs.push(() => runOne(q, w, mcpPaths[w.id]));
  });
  console.log(`run ${runId}: ${jobs.length} sessions, concurrency ${concurrency}`);
  const records = await pool(jobs, concurrency);
  const failed = records.filter((r) => r?.error);
  for (const f of failed) console.error('job error:', f.error);
  const cost = records.reduce((s, r) => s + (r?.cost_usd ?? 0), 0);
  console.log(`done: ${records.length - failed.length} records, worker cost $${cost.toFixed(2)}`);
  // The checkouts must be unmodified after the run (workers are told they are read-only).
  for (const p of corpus) {
    const dirty = execFileSync('git', ['-C', p, 'status', '--porcelain']).toString().trim();
    if (dirty) console.error(`WARNING: checkout modified during run: ${p}\n${dirty.slice(0, 500)}`);
  }
}

main().catch((e) => { console.error(e); process.exit(1); });
