// Shared helpers for the unified benchmark: paths, question/worker loading, headless
// Claude Code spawning, stream-json parsing (per-request token usage), bounded pools.
//
//   node lib.mjs --self-test     parser + token-accounting self-test (no network)
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const UNIFIED_DIR = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(UNIFIED_DIR, '../../../..');
export const CORPUS_ROOT = path.join(REPO_ROOT, 'octocode-local-testing/repos');
export const WORKERS_DIR = path.join(UNIFIED_DIR, 'workers');
export const QUESTIONS_DIR = path.join(UNIFIED_DIR, 'questions');
export const REFERENCES_DIR = path.join(UNIFIED_DIR, 'references');
export const RESULTS_DIR = path.join(UNIFIED_DIR, 'results');
export const HARNESS_FILES = ['lib.mjs', 'run.mjs', 'judge.mjs', 'report.mjs', 'reflect.mjs', 'REFLECT.md'];

export const sha256 = (s) => createHash('sha256').update(s).digest('hex');
export const hashFile = (p) => { try { return sha256(fs.readFileSync(p)); } catch { return null; } };

export function parseArgs(argv, defaults = {}) {
  const out = { ...defaults };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) continue;
    const key = a.slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) out[key] = true;
    else { out[key] = next; i++; }
  }
  return out;
}

export function writeJson(file, data) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + '\n');
}
export const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));

// ---------------------------------------------------------------- questions

export function loadQuestions() {
  const qs = readJson(path.join(QUESTIONS_DIR, 'questions.json')).questions;
  for (const q of qs) for (const r of q.repos ?? []) if (r.dir) r.path = path.join(CORPUS_ROOT, r.dir);
  return qs;
}

export function selectQuestions(all, spec) {
  if (!spec || spec === 'all') return all;
  const ids = String(spec).split(',');
  return all.filter((q) => ids.includes(q.id));
}

/** Every local checkout used by the question set. All workers get exactly these. */
export const corpusPaths = (all) => [...new Set(all.flatMap((q) => (q.repos ?? []).filter((r) => r.path).map((r) => r.path)))].sort();

/** The user prompt. Identical for every worker: question, plus the checkout path(s) for local questions. */
export function buildPrompt(q) {
  const parts = [q.question.trim()];
  const local = (q.repos ?? []).filter((r) => r.path);
  if (local.length) {
    parts.push('Local checkout (read-only, at the pinned commit):\n' + local.map((r) => `- ${r.repo} @ ${r.sha}: ${r.path}`).join('\n'));
  }
  return parts.join('\n\n');
}

// ---------------------------------------------------------------- workers

/**
 * A worker is workers/<id>/WORKER.md (instructions: the subject under test) plus
 * workers/<id>/profile.json (tool wiring). The harness never branches on the id.
 *
 * profile.json:
 *   tools            string   value for `--tools` ("" = no built-in tools)
 *   allowedTools     string[] `--allowedTools` entries
 *   disallowedTools  string[] `--disallowedTools` entries (optional)
 *   mcpServers       object   MCP servers ({} = none). Strings may use ${REPO_ROOT}, ${CORPUS_ROOT}, ${CORPUS_PATHS}
 *   addCorpusDirs    boolean  pass every corpus checkout via `--add-dir`
 *   isolation        { allowedToolPattern: regex source every offered/called tool must match,
 *                      requiredMcpServers: string[], forbidMcpServers: boolean }
 *   counters         [{ label, tool?: regex source, inputKey?: string }]  usage counters for the report
 */
export function loadWorkers(filter) {
  const ids = fs.readdirSync(WORKERS_DIR).filter((d) => fs.existsSync(path.join(WORKERS_DIR, d, 'profile.json'))).sort();
  const wanted = !filter || filter === 'all' ? ids : String(filter).split(',');
  return wanted.map((id) => {
    const dir = path.join(WORKERS_DIR, id);
    const docPath = path.join(dir, 'WORKER.md');
    const profilePath = path.join(dir, 'profile.json');
    if (!fs.existsSync(docPath)) throw new Error(`worker ${id}: missing WORKER.md`);
    return { id, dir, docPath, profilePath, profile: readJson(profilePath), docSha: hashFile(docPath), profileSha: hashFile(profilePath) };
  });
}

function expand(value, vars) {
  if (typeof value === 'string') return value.replace(/\$\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
  if (Array.isArray(value)) return value.map((v) => expand(v, vars));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, expand(v, vars)]));
  return value;
}

/** Build the claude CLI flags for a worker. mcpConfigPath is written by the caller when mcpServers is non-empty. */
export function workerFlags(worker, corpus, mcpConfigPath) {
  const p = worker.profile;
  const flags = ['--strict-mcp-config', '--tools', p.tools ?? ''];
  const servers = mcpServersFor(worker, corpus);
  if (Object.keys(servers).length) flags.push('--mcp-config', mcpConfigPath);
  if (p.allowedTools?.length) flags.push('--allowedTools', ...p.allowedTools);
  if (p.disallowedTools?.length) flags.push('--disallowedTools', ...p.disallowedTools);
  if (p.addCorpusDirs) flags.push('--add-dir', ...corpus);
  return flags;
}

export function mcpServersFor(worker, corpus) {
  return expand(worker.profile.mcpServers ?? {}, { REPO_ROOT, CORPUS_ROOT, CORPUS_PATHS: corpus.join(',') });
}

/** Flags shared by every worker and the judge: clean lab, no settings/memory/skills, headless, stream-json. */
/** persist=true keeps the session on disk (in its fresh cwd's project folder) so the reflection step can resume it. */
export function commonFlags({ model, maxTurns, persist = false }) {
  return [
    '--model', model, '--setting-sources', '', '--max-turns', String(maxTurns),
    '--output-format', 'stream-json', '--verbose',
    ...(persist ? [] : ['--no-session-persistence']), '--disable-slash-commands', '--permission-prompts', 'none',
  ];
}

/** A fresh empty working directory outside the repository (no CLAUDE.md, no project memory). */
export function freshCwd(label) {
  return fs.mkdtempSync(path.join(os.tmpdir(), `ocbench-${label.replace(/[^\w-]/g, '_')}-`));
}

/** Run `claude` and tee stdout (stream-json) to streamPath. */
export function runClaude({ args, cwd, timeoutMs, streamPath, env = process.env }) {
  return new Promise((resolve) => {
    const start = Date.now();
    const out = fs.createWriteStream(streamPath);
    const child = spawn('claude', args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; child.kill('SIGTERM'); setTimeout(() => child.kill('SIGKILL'), 5000); }, timeoutMs);
    child.stdout.on('data', (d) => { stdout += d; out.write(d); });
    child.stderr.on('data', (d) => { stderr += d; });
    child.on('close', (code) => {
      clearTimeout(timer);
      out.end();
      resolve({ stream: stdout, stderr, exitCode: code, timedOut, wallMs: Date.now() - start });
    });
  });
}

// ---------------------------------------------------------------- stream parsing + tokens

const USAGE_KEYS = ['input_tokens', 'cache_creation_input_tokens', 'cache_read_input_tokens', 'output_tokens'];

/**
 * Parse a stream-json transcript.
 * Per-request usage: every API request is one assistant message id; stream-json may emit one
 * event per content block with the same id, so usage is merged per id (max of each field).
 */
export function parseStream(stream) {
  const events = [];
  for (const line of String(stream).split('\n')) {
    if (!line.trim()) continue;
    try { events.push(JSON.parse(line)); } catch { /* partial line on kill */ }
  }
  const init = events.find((e) => e.type === 'system' && e.subtype === 'init') ?? null;
  const result = [...events].reverse().find((e) => e.type === 'result') ?? null;
  const requests = new Map();
  const toolCalls = [];
  const toolErrors = [];
  let lastAssistantText = '';
  for (const e of events) {
    if (e.type === 'assistant' && e.message) {
      if (e.parent_tool_use_id) continue; // sub-agent traffic (none expected) is not the worker's own request
      const id = e.message.id ?? `anon-${requests.size}`;
      const u = e.message.usage ?? {};
      const prev = requests.get(id) ?? { id, model: e.message.model, ...Object.fromEntries(USAGE_KEYS.map((k) => [k, 0])) };
      for (const k of USAGE_KEYS) prev[k] = Math.max(prev[k], Number(u[k] ?? 0));
      requests.set(id, prev);
      const texts = [];
      for (const c of e.message.content ?? []) {
        if (c.type === 'tool_use') toolCalls.push({ id: c.id, name: c.name, input: c.input ?? {} });
        else if (c.type === 'text') texts.push(c.text);
      }
      if (texts.length) lastAssistantText = texts.join('\n');
    } else if (e.type === 'user') {
      for (const c of e.message?.content ?? []) {
        if (c.type === 'tool_result' && c.is_error) {
          const text = typeof c.content === 'string' ? c.content : JSON.stringify(c.content);
          toolErrors.push({ tool_use_id: c.tool_use_id, text: text.slice(0, 400) });
        }
      }
    }
  }
  const perRequest = [...requests.values()];
  return {
    sessionId: init?.session_id ?? result?.session_id ?? null,
    init: init ? { tools: init.tools ?? [], mcp_servers: init.mcp_servers ?? [], model: init.model, permissionMode: init.permissionMode, cwd: init.cwd, skills: init.skills ?? [], slash_commands: init.slash_commands ?? [] } : null,
    answer: result?.result || lastAssistantText || '',
    resultSubtype: result?.subtype ?? null,
    isError: result?.is_error ?? true,
    resultUsage: result?.usage ? Object.fromEntries(USAGE_KEYS.map((k) => [k, Number(result.usage[k] ?? 0)])) : null,
    modelUsage: result?.modelUsage ?? null,
    total_cost_usd: result?.total_cost_usd ?? 0,
    duration_ms: result?.duration_ms ?? 0,
    num_turns: result?.num_turns ?? 0,
    permission_denials: result?.permission_denials ?? [],
    perRequest,
    toolCalls,
    toolErrors,
  };
}

/**
 * Token accounting for one run (see README "How tokens are computed").
 *   context_i  = input + cache_creation + cache_read of request i (everything the model processed)
 *   total      = Σ context_i + Σ output_i
 *   overhead   = context_1 (system prompt + tool definitions + worker doc + question) × requests
 *   research   = total − overhead
 */
export function tokenAccounting(perRequest) {
  const ctx = (r) => r.input_tokens + r.cache_creation_input_tokens + r.cache_read_input_tokens;
  const sum = (k) => perRequest.reduce((s, r) => s + r[k], 0);
  const requests = perRequest.length;
  const context = perRequest.reduce((s, r) => s + ctx(r), 0);
  const output = sum('output_tokens');
  const firstContext = requests ? ctx(perRequest[0]) : 0;
  const overhead = firstContext * requests;
  const total = context + output;
  return {
    requests,
    input_tokens: sum('input_tokens'),
    cache_creation_input_tokens: sum('cache_creation_input_tokens'),
    cache_read_input_tokens: sum('cache_read_input_tokens'),
    output_tokens: output,
    context_tokens: context,
    total_tokens: total,
    first_request_context: firstContext,
    fixed_overhead_tokens: overhead,
    research_tokens: total - overhead,
    max_request_context: perRequest.reduce((m, r) => Math.max(m, ctx(r)), 0),
  };
}

/** Group tool calls by name; Bash calls by their first command word (rg, gh, ...). */
export function toolCounts(toolCalls) {
  const counts = {};
  for (const c of toolCalls) {
    const key = c.name === 'Bash' ? `Bash:${/^\s*([\w.-]+)/.exec(String(c.input?.command ?? ''))?.[1] ?? 'other'}` : c.name;
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}

/** Count occurrences of an input key anywhere in a call's input (top level or inside queries[]). */
export function countInputKey(input, key) {
  let n = 0;
  const walk = (v) => {
    if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === 'object') for (const [k, x] of Object.entries(v)) { if (k === key && x !== undefined && x !== null && x !== '') n++; walk(x); }
  };
  walk(input);
  return n;
}

/** Profile counters: [{label, tool?, inputKey?}] → {label: count}. */
export function applyCounters(counters = [], toolCalls) {
  const out = {};
  for (const c of counters) {
    const re = c.tool ? new RegExp(c.tool) : null;
    let n = 0;
    for (const call of toolCalls) {
      if (re && !re.test(call.name)) continue;
      n += c.inputKey ? countInputKey(call.input, c.inputKey) : 1;
    }
    out[c.label] = n;
  }
  return out;
}

/** Isolation check against the worker profile, using what the session actually offered and called. */
export function isolationCheck(profile, m) {
  const iso = profile.isolation ?? {};
  const problems = [];
  const offered = m.init?.tools ?? [];
  const servers = (m.init?.mcp_servers ?? []).map((s) => ({ name: s.name, status: s.status }));
  const allow = iso.allowedToolPattern ? new RegExp(iso.allowedToolPattern) : null;
  if (!m.init) problems.push('no init event');
  if (allow) {
    const bad = offered.filter((t) => !allow.test(t));
    if (bad.length) problems.push(`tools offered outside profile: ${bad.join(',')}`);
    const badCalls = m.toolCalls.filter((c) => !allow.test(c.name)).map((c) => c.name);
    if (badCalls.length) problems.push(`tool calls outside profile: ${[...new Set(badCalls)].join(',')}`);
  }
  for (const s of iso.requiredMcpServers ?? []) {
    if (!servers.some((x) => x.name === s && x.status === 'connected')) problems.push(`MCP server ${s} not connected (${JSON.stringify(servers)})`);
  }
  if (iso.forbidMcpServers && servers.length) problems.push(`MCP servers present: ${servers.map((s) => s.name)}`);
  if ((m.init?.skills ?? []).length) problems.push(`skills offered: ${m.init.skills.length}`);
  // Contamination: any tool input that touches the benchmark package (references, results, questions).
  const touched = m.toolCalls.filter((c) => /octocode-benchmark|compare\/unified|references\/[GL]\d/.test(JSON.stringify(c.input)));
  if (touched.length) problems.push(`tool input touched benchmark files: ${touched.map((c) => c.name).join(',')}`);
  return { ok: problems.length === 0, problems, offeredTools: offered, mcpServers: servers };
}

// ---------------------------------------------------------------- pool

/** Run async job factories with at most `limit` in flight. */
export async function pool(jobs, limit) {
  const results = new Array(jobs.length);
  let next = 0;
  async function worker() {
    while (next < jobs.length) {
      const i = next++;
      try { results[i] = await jobs[i](); } catch (err) { results[i] = { error: String(err?.stack ?? err) }; }
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, jobs.length) }, worker));
  return results;
}

export function median(xs) {
  const a = xs.filter((x) => Number.isFinite(x)).sort((x, y) => x - y);
  if (!a.length) return null;
  const m = Math.floor(a.length / 2);
  return a.length % 2 ? a[m] : (a[m - 1] + a[m]) / 2;
}
export const mean = (xs) => { const a = xs.filter((x) => Number.isFinite(x)); return a.length ? a.reduce((s, x) => s + x, 0) / a.length : null; };

// ---------------------------------------------------------------- self-test

function selfTest() {
  const ev = (o) => JSON.stringify(o);
  const u = (i, cc, cr, o) => ({ input_tokens: i, cache_creation_input_tokens: cc, cache_read_input_tokens: cr, output_tokens: o });
  const stream = [
    ev({ type: 'system', subtype: 'init', tools: ['mcp__x__a'], mcp_servers: [{ name: 'x', status: 'connected' }] }),
    ev({ type: 'assistant', message: { id: 'm1', usage: u(10, 1000, 0, 5), content: [{ type: 'text', text: 'hi' }] } }),
    ev({ type: 'assistant', message: { id: 'm1', usage: u(10, 1000, 0, 40), content: [{ type: 'tool_use', id: 't1', name: 'mcp__x__a', input: { queries: [{ matchString: 'foo' }, { matchString: 'bar' }] } }] } }),
    ev({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't1', is_error: true, content: 'boom' }] } }),
    ev({ type: 'assistant', message: { id: 'm2', usage: u(5, 200, 1000, 60), content: [{ type: 'text', text: 'final' }] } }),
    ev({ type: 'result', subtype: 'success', is_error: false, result: 'final', total_cost_usd: 0.01, num_turns: 2, usage: u(15, 1200, 1000, 100) }),
    '{"partial',
  ].join('\n');
  const m = parseStream(stream);
  const t = tokenAccounting(m.perRequest);
  const assert = (c, msg) => { if (!c) { console.error('FAIL', msg); process.exit(1); } };
  assert(m.perRequest.length === 2, 'two requests');
  assert(t.output_tokens === 100, `output 100 got ${t.output_tokens}`);
  assert(t.context_tokens === 1010 + 1205, 'context');
  assert(t.total_tokens === 2315, 'total');
  assert(t.fixed_overhead_tokens === 2020 && t.research_tokens === 295, 'overhead/research');
  assert(m.toolErrors.length === 1 && m.answer === 'final', 'errors/answer');
  assert(applyCounters([{ label: 'ms', inputKey: 'matchString' }, { label: 'a', tool: '__a$' }], m.toolCalls).ms === 2, 'counter');
  assert(isolationCheck({ isolation: { allowedToolPattern: '^mcp__x__', requiredMcpServers: ['x'] } }, m).ok, 'isolation ok');
  assert(!isolationCheck({ isolation: { allowedToolPattern: '^Bash$', forbidMcpServers: true } }, m).ok, 'isolation catches');
  assert(buildPrompt({ question: 'Q?', repos: [{ repo: 'o/r', sha: 's', path: '/x' }] }).includes('/x'), 'prompt');
  console.log('lib self-test: ok');
}

if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv.includes('--self-test')) selfTest();
