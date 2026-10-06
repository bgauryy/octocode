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
import { solverBoundary, evaluatorCredentials, ISOLATION_VERSION, MODEL_HOSTS } from './isolation.mjs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { getOctocodeHome, propagateOctocodeEnv } from '@octocodeai/config';
import {
  CORPUS_ROOT, HARNESS_FILES, REFERENCES_DIR, REPO_ROOT, RESULTS_DIR, UNIFIED_DIR, applyCounters, buildPrompt,
  commonFlags, configurationHashes, corpusPaths, freshCwd, hashFile, isolationCheck, loadQuestions, loadWorkers, mcpServersFor, parseArgs, sessionServerEnv,
  parseStream, pool, readJson, runClaude, selectQuestions, sha256, tokenAccounting, toolCounts, workerFlags, writeJson, stopChildren, hashTree, recordedProbe, failedProbeDetails, resumedAccounting,
} from './lib.mjs';

const args = parseArgs(process.argv.slice(2), {
  questions: 'all', workers: 'all', concurrency: '4', 'max-turns': '40', 'timeout-min': '20', model: 'sonnet',
});
if (!args['run-id'] || !/^[\w-]+$/.test(String(args['run-id']))) throw new Error('valid --run-id is required');
if (!/^(claude-|[a-z].*-\d)/.test(String(args.model))) throw new Error('Use a concrete versioned --model, not a rolling alias.');
const runId = String(args['run-id']);
const runDir = path.join(RESULTS_DIR, runId);
const concurrency = Math.min(4, Number(args.concurrency)); // hard cap: at most 4 worker processes
const maxTurns = Number(args['max-turns']);
const timeoutMs = Number(args['timeout-min']) * 60_000;
const model = String(args.model);

if (![concurrency, maxTurns, timeoutMs].every(n => Number.isFinite(n) && n > 0)) throw new Error('positive finite limits required');
const all = loadQuestions();
const questions = selectQuestions(all, args.questions);
const workers = loadWorkers(args.workers);
if (!questions.length || !workers.length) throw new Error('empty selection');
const corpus = corpusPaths(all);
let credentials;
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
  const effectiveEnv = { ...process.env };
  propagateOctocodeEnv({ cwd: REPO_ROOT, env: effectiveEnv });
  return {
    runId,
    createdAt: new Date().toISOString(),
    host: `${os.platform()}-${os.arch()}`,
    claudeVersion: execFileSync('claude', ['--version']).toString().trim(),
    executables: Object.fromEntries(['claude', 'gh', 'node'].map(name => { const file = fs.realpathSync(execFileSync('/usr/bin/which', [name], { encoding: 'utf8' }).trim()); return [name, { file, sha256: hashFile(file) }]; })),
    model, maxTurns, timeoutMs, concurrency,
    isolation: { version: ISOLATION_VERSION, modelHosts: MODEL_HOSTS, github: 'REST GET/HEAD only' },
    workers: workers.map((w) => w.id),
    questionIds: questions.map((q) => q.id),
    repo: { head: git('rev-parse', 'HEAD'), branch: git('rev-parse', '--abbrev-ref', 'HEAD'), dirtyFiles: (git('status', '--porcelain') ?? '').split('\n').filter(Boolean).length },
    corpus: [...new Map(all.flatMap((q) => q.repos ?? []).filter((r) => r.path).map((r) => [r.path, r])).values()].map((r) => ({
      repo: r.repo, dir: r.dir, sha: r.sha, head: execFileSync('git', ['-C', r.path, 'rev-parse', 'HEAD']).toString().trim(),
      status: execFileSync('git', ['-C', r.path, 'status', '--porcelain', '--untracked-files=all']).toString().trim(),
      diff: sha256(execFileSync('git', ['-C', r.path, 'diff', 'HEAD', '--binary'])),
      content: hashTree(r.path),
    })),
    build: {
      mcpServerDist: hashFile(MCP_DIST),
      nativeAddons: nativeArtifacts(),
      fingerprint: readBuildFingerprint(),
      mcpDistTree: hashTree(path.join(REPO_ROOT, 'packages/octocode-mcp/dist')),
      configDistTree: hashTree(path.join(REPO_ROOT, 'packages/octocode-config/dist')),
      coreResolvedTree: hashTree(fs.realpathSync(path.join(REPO_ROOT, 'node_modules/@octocodeai/octocode-core'))),
      runtimeDependencies: hashTree(path.join(REPO_ROOT, 'node_modules')),
    },
    hashes: {
      harness: Object.fromEntries(HARNESS_FILES.map((f) => [f, hashFile(path.join(UNIFIED_DIR, f))])),
      questionsJson: hashFile(path.join(UNIFIED_DIR, 'questions/questions.json')),
      references: Object.fromEntries(questions.map(q => [q.id, hashFile(path.join(REFERENCES_DIR, `${q.id}.md`))])),
      dependencyLock: hashFile(path.join(REPO_ROOT, 'yarn.lock')),
      configDist: hashFile(path.join(REPO_ROOT, 'packages/octocode-config/dist/index.js')),
      environment: sha256(JSON.stringify(Object.entries(effectiveEnv).filter(([k]) => /^(OCTOCODE_|TOOLS_TO_RUN|DISABLE_TOOLS|GITHUB_API_URL|REQUEST_TIMEOUT|MAX_RETRIES)/.test(k)).sort())),
      workers: Object.fromEntries(workers.map((w) => [w.id, { doc: w.docSha, profile: w.profileSha }])),
      ...configurationHashes(REPO_ROOT, getOctocodeHome()),
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

/** `next` with `prev`'s question/worker selection merged in: a resume may
 * select a subset, so only a changed input (or a new question/worker) differs. */
function withSelection(next, prev) {
  next.createdAt = prev.createdAt;
  next.questionIds = [...new Set([...prev.questionIds, ...next.questionIds])];
  next.workers = [...new Set([...prev.workers, ...next.workers])];
  for (const key of ['prompts', 'references', 'workers']) next.hashes[key] = { ...prev.hashes[key], ...next.hashes[key] };
  return next;
}
const FROZEN = m => JSON.stringify({ hashes: m.hashes, build: m.build, model: m.model, maxTurns: m.maxTurns, timeoutMs: m.timeoutMs, claudeVersion: m.claudeVersion, executables: m.executables, corpus: m.corpus, isolation: m.isolation, workers: m.workers, questionIds: m.questionIds });

function workerMcpConfig(worker) {
  const servers = mcpServersFor(worker, corpus);
  if (!Object.keys(servers).length) return null;
  const p = path.join(runDir, 'config', `${worker.id}.mcp.json`);
  writeJson(p, { mcpServers: servers });
  return p;
}

const REFLECT_PROMPT = fs.readFileSync(path.join(UNIFIED_DIR, 'REFLECT.md'), 'utf8');

async function session({ dir, prompt, worker, label, turns = maxTurns, timeout = timeoutMs, reflect = false, serverEnv = {} }) {
  fs.mkdirSync(dir, { recursive: true });
  const cwd = freshCwd(label);
  let boundary;
  let completed;
  try {
    const hasMcp = Object.keys(worker.profile.mcpServers ?? {}).length > 0;
    const configured = hasMcp ? mcpServersFor(worker, corpus).octocode ?? null : null;
    const mcpServer = configured && { ...configured, env: { ...configured.env, ...serverEnv } };
    boundary = await solverBoundary({ cwd, corpus, repoRoot: REPO_ROOT, mcp: hasMcp, mcpServer, statsHome: path.join(dir, 'native-home'), ...credentials });
    const doc = path.join(cwd, 'WORKER.md');
    fs.copyFileSync(worker.docPath, doc);
    const profile = structuredClone(worker);
    if (hasMcp) profile.profile.mcpServers = { octocode: { command: process.execPath, args: [boundary.bridge] } };
    const config = path.join(cwd, 'mcp.json');
    writeJson(config, { mcpServers: profile.profile.mcpServers ?? {} });
    const toolFlags = ['--append-system-prompt-file', doc, ...workerFlags(profile, corpus, config)];
    const started = new Date().toISOString();
    const res = await runClaude({ args: ['-p', prompt, ...commonFlags({ model, maxTurns: turns, persist: reflect }), ...toolFlags], cwd, timeoutMs: timeout, streamPath: path.join(dir, 'stream.jsonl'), env: boundary.env, sandboxProfile: boundary.sandboxProfile });
    fs.writeFileSync(path.join(dir, 'stderr.txt'), res.stderr);
    const m = parseStream(res.stream);
    let reflection = null;
    if (reflect && m.sessionId && res.exitCode === 0 && !m.isError) {
      const r = await runClaude({ args: ['-p', REFLECT_PROMPT, '--resume', m.sessionId, ...commonFlags({ model, maxTurns: 1, persist: true }), '--strict-mcp-config', '--tools', ''], cwd, timeoutMs: 300000, streamPath: path.join(dir, 'reflect.stream.jsonl'), env: boundary.env, sandboxProfile: boundary.sandboxProfile });
      const rm = parseStream(r.stream);
      const accounting = resumedAccounting(m, rm);
      reflection = { text: rm.answer, tokens: accounting.tokens, cost_usd: accounting.cost_usd, costVerified: accounting.costVerified, accounting, toolCallsAttempted: rm.toolCalls.length, valid: r.exitCode === 0 && !r.signal && !r.timedOut && !rm.isError && rm.resultSubtype === 'success' && accounting.verified && rm.toolCalls.length === 0 };
      fs.writeFileSync(path.join(dir, 'reflection.md'), rm.answer ?? '');
    }
    completed = { res, m, started, reflection };
  } finally {
    await boundary?.close();
    if (completed) Object.assign(completed, { classificationProvider: boundary.providerStats(), nativeCalls: boundary.nativeCalls(), gatewayTraffic: boundary.traffic(), nativeGithubUsage: boundary.githubStats() });
    fs.rmSync(cwd, { recursive: true, force: true });
  }
  return completed;
}

function summarize({ q, worker, res, m, started, prompt, dir }) {
  const tokens = tokenAccounting(m);
  return {
    qid: q?.id ?? null, worker: worker.id, started,
    promptSha256: sha256(prompt), workerDocSha256: worker.docSha,
    exitCode: res.exitCode, signal: res.signal, timedOut: res.timedOut, wallMs: res.wallMs,
    status: res.timedOut ? 'timeout' : res.exitCode === 0 && !res.signal && tokens.verified && m.resultSubtype === 'success' && !m.isError ? 'ok' : (m.resultSubtype ?? 'no-result'),
    answer: m.answer,
    tokens,
    perRequest: m.perRequest.map(({ id, ...u }) => u),
    resultUsage: m.resultUsage,
    modelUsage: m.modelUsage,
    cost_usd: m.total_cost_usd,
    costVerified: m.costVerified,
    duration_ms: m.duration_ms,
    num_turns: m.num_turns,
    toolCallCount: m.toolCalls.length,
    toolCounts: toolCounts(m.toolCalls),
    counters: applyCounters(worker.profile.counters, m.toolCalls),
    toolCalls: m.toolCalls.map((c) => ({ name: c.name, input: JSON.stringify(c.input).slice(0, 600) })),
    toolErrorCount: m.toolErrors.length, rowErrorCount: m.rowErrors.length,
    actualModels: m.actualModels, providerCostIncluded: false,
    providerUsageStatus: m.toolCalls.some(c => /clasify$/.test(c.name)) ? 'unknown; classifier provider telemetry required' : 'no classifier calls',
    toolErrors: m.toolErrors.slice(0, 20),
    permission_denials: m.permission_denials.map((d) => ({ tool: d.tool_name, input: JSON.stringify(d.tool_input).slice(0, 300) })),
    isolation: isolationCheck(worker.profile, m, { ownDirs: [path.join(dir, 'native-home')] }),
  };
}

async function runOne(q, worker, mcpPath) {
  const dir = path.join(runDir, 'runs', q.id, worker.id);
  const recordPath = path.join(dir, 'run.json');
  if (fs.existsSync(recordPath)) return readJson(recordPath);
  const prompt = buildPrompt(q);
  const serverEnv = sessionServerEnv(worker.profile, q);
  const { res, m, started, reflection, classificationProvider, nativeCalls, gatewayTraffic, nativeGithubUsage } = await session({ dir, prompt, worker, mcpPath, label: `${q.id}-${worker.id}`, reflect: true, serverEnv });
  const record = summarize({ q, worker, res, m, started, prompt, dir });
  record.reflection = reflection;
  if (Object.keys(serverEnv).length) record.serverEnv = serverEnv;
  record.classificationProvider = classificationProvider;
  record.nativeCalls = nativeCalls;
  record.gatewayTraffic = gatewayTraffic;
  record.nativeGithubUsage = nativeGithubUsage;
  record.networkAccounting = { scope: 'isolated session including tools-disabled reflection', gateway: 'forwarded REST GET/HEAD HTTP attempts; response statuses/failures retained', nativeGithub: nativeGithubUsage ? 'observed native session aggregate; retain its completeness flag' : 'unknown; native aggregate absent', model: 'CONNECT tunnels, not physical model HTTP requests' };
  record.nativeRowErrorCount = nativeCalls.reduce((sum, c) => sum + c.rowErrors.length, 0);
  record.rowErrorCount = Math.max(record.rowErrorCount, record.nativeRowErrorCount);
  const classifierIds = new Set(m.toolCalls.filter(c => /clasify$/.test(c.name)).map(c => c.id));
  const classifierError = [...m.rowErrors, ...m.toolErrors].some(e => classifierIds.has(e.tool_use_id)) || nativeCalls.some(c => c.tool === 'clasify' && (c.rowErrors.length || c.transportError || c.isError));
  const validStats = classificationProvider && ['calls', 'input_tokens', 'output_tokens', 'known_usage_calls', 'unknown_usage_calls'].every(k => Number.isSafeInteger(classificationProvider[k]) && classificationProvider[k] >= 0);
  record.classificationAccounting = { observed: classificationProvider, groupedToolCalls: classifierIds.size, failedToolOrRows: classifierError,
    reportedSuccessfulUsageComplete: !!validStats && classificationProvider.calls === classificationProvider.known_usage_calls + classificationProvider.unknown_usage_calls && classificationProvider.unknown_usage_calls === 0 && !classifierError,
    allProviderAttemptsVerified: false, cost_usd: null, reason: 'Native aggregate reports successful provider usage; failed billed attempts and provider tariff are not independently available. Cached tool calls and grouped questions do not equal provider requests.' };
  record.providerUsageStatus = classificationProvider ? 'observed native provider aggregate; failed requests/cost require separate verification' : record.providerUsageStatus;
  record.valid = record.status === 'ok' && record.costVerified && record.isolation.ok && (!reflection || reflection.valid);
  // Defense in depth: inspect tool results in addition to the OS boundary.
  const toolResults = fs.readFileSync(path.join(dir, 'stream.jsonl'), 'utf8').split('\n').filter((l) => l.includes('"tool_result"')).join('\n');
  if (toolResults.includes(REF_CANARY) || /# [GL]\d\d reference/.test(toolResults)) {
    record.isolation.ok = false;
    record.isolation.problems.push('answer-key text appeared in a tool result (run invalid)');
    record.valid = false;
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
  const record = await recordedProbe({ recordPath, worker: worker.id, probe: name, secrets: [...Object.values(credentials), ...Object.entries(process.env).filter(([key]) => /TOKEN|SECRET|PASSWORD|API_KEY/.test(key)).map(([, value]) => value)] }, async () => {
    if (fs.existsSync(recordPath)) return readJson(recordPath);
    const { prompt, turns } = PROBES[name];
    const { res, m, started, nativeCalls, classificationProvider, gatewayTraffic, nativeGithubUsage } = await session({ dir, prompt, worker, mcpPath, label: `probe-${name}-${worker.id}`, turns, timeout: 5 * 60_000 });
    const record = summarize({ q: null, worker, res, m, started, prompt, dir });
    record.nativeCalls = nativeCalls; record.classificationProvider = classificationProvider;
    record.gatewayTraffic = gatewayTraffic; record.nativeGithubUsage = nativeGithubUsage;
    record.nativeRowErrorCount = nativeCalls.reduce((sum, c) => sum + c.rowErrors.length, 0);
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
    return record;
  });
  console.log(`probe ${name} ${worker.id}: ${record.status} first-request context=${record.tokens?.first_request_context ?? 'unknown'} isolation=${record.isolation.ok ? 'ok' : record.isolation.problems.join(';')}${record.leak ? ' leak=' + JSON.stringify(record.leak) : ''}${record.error ? ' error=' + JSON.stringify(record.error) : ''}`);
  return record;
}

for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => {
  stopChildren(); process.exitCode = 130;
  setTimeout(() => { stopChildren('SIGKILL'); process.exit(130); }, 5000).unref();
});
async function main() {
  fs.mkdirSync(runDir, { recursive: true });
  credentials = evaluatorCredentials();
  await workerPhase();
}

async function workerPhase() {
  const manifestPath = path.join(runDir, 'manifest.json');
  const manifest = buildManifest();
  for (const r of manifest.corpus) if (r.status || (!r.head.startsWith(r.sha) && r.sha !== r.head)) throw new Error(`corpus ${r.dir} is at ${r.head}, expected ${r.sha}`);
  if (!fs.existsSync(MCP_DIST)) throw new Error(`MCP build missing: ${MCP_DIST}`);
  if (fs.existsSync(manifestPath)) {
    const prev = readJson(manifestPath);
    withSelection(manifest, prev);
    if (FROZEN(prev) !== FROZEN(manifest)) throw new Error('harness, questions, worker docs/profiles or MCP build changed since this run started; use a new --run-id');
  }
  writeJson(manifestPath, manifest);
  const mcpPaths = Object.fromEntries(workers.map((w) => [w.id, workerMcpConfig(w)]));

  if (args.probes || args['probes-only']) {
    const probeJobs = workers.flatMap((w) => Object.keys(PROBES).map((p) => () => runProbe(w, p, mcpPaths[w.id])));
    const probes = await pool(probeJobs, concurrency);
    const failures = failedProbeDetails(probes);
    writeJson(path.join(runDir, 'probes', 'summary.json'), { expected: probeJobs.length, completed: probes.length, failures });
    if (failures.length || probes.length !== probeJobs.length) throw new Error(`probe gate failed: ${JSON.stringify(failures)}`);
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
  const failed = records.filter(r => r?.error || !r.valid);
  for (const f of failed) console.error('job error:', f.error);
  const cost = records.reduce((s, r) => s + (r?.cost_usd ?? 0), 0);
  console.log(`done: ${records.length - failed.length} records, worker cost $${cost.toFixed(2)}`);
  // The checkouts must be unmodified after the run (workers are told they are read-only).
  for (const p of corpus) {
    const dirty = execFileSync('git', ['-C', p, 'status', '--porcelain']).toString().trim();
    if (dirty) throw new Error(`checkout modified during run: ${p}`);
  }
  if (FROZEN(manifest) !== FROZEN(withSelection(buildManifest(), manifest))) throw new Error('frozen inputs changed during run');
  if (failed.length || records.length !== jobs.length) throw new Error(`${failed.length} invalid/incomplete worker records`);
}

main().catch((e) => { console.error(e); process.exit(1); });
