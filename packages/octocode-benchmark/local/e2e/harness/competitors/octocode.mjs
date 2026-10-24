// The octocode side of a competitor task: one MCP server per workspace (CLI for
// CLI-only tools), the recipe's steps (call, follow a continuation, walk pages)
// with `{{prev:…}}`/`{{hint:…}}` templates, evidence + sensors per run, and the
// verbatim replay of offered continuations.
import path from 'node:path';
import { REPOS_REL, ROOT } from '../paths.mjs';
import { runCli, startServer } from '../mcp-client.mjs';
import { nextHints } from '../responses.mjs';
import { canon } from '../lead-predicates.mjs';
import { callRows, schemaErrors } from '../sensors.mjs';
import { sleep } from '../util.mjs';
import { octocodeEvidence } from './octocode-evidence.mjs';
import { taskSensors, stepBytes } from './response-sensors.mjs';
import { emptyEvidence, evaluate, evidenceCounts } from './truth.mjs';
import { NORMALIZED, RATE_LIMIT, relativize } from './workspace.mjs';

// Real agent briefs run about 14–32 chars (mainGoal) and 11–25 (reasoning).
const REASONING = NORMALIZED ? 'Need line-level evidence' : 'Answer the task question.';
/** Continuations replayed verbatim per task (0 = off). */
export const REPLAY_MAX = Number(process.env.OCTOCODE_COMPETITOR_REPLAY_MAX ?? 3);
const TRANSIENT = /MCP server|server unavailable|exited|timeout tools\/call|fingerprint|ECONNRESET|suite MCP deadline/i;
const surfaceEnv = (ws, root) => ({ OCTOCODE_BETA: 'true', OCTOCODE_RANKING_ENABLED: 'true', ...(ws ? { WORKSPACE_ROOT: root } : {}) });

// ---------- servers ----------
// One MCP server per workspace (the octocode root, or a corpus repo root).
const clients = new Map();
export async function clientFor(ws) {
  if (!clients.has(ws)) {
    const root = path.join(ROOT, ws);
    clients.set(ws, await startServer({ cwd: root, env: surfaceEnv(ws, root) }));
  }
  return clients.get(ws);
}
async function restart(ws) {
  clients.get(ws)?.close();
  clients.delete(ws);
  return clientFor(ws);
}
export function closeClients() {
  for (const c of clients.values()) c.close();
}

let lastCodeSearch = 0;
/** GitHub code search allows 10 calls a minute: space them 7 s apart. */
export async function paceCodeSearch() {
  const wait = lastCodeSearch + 7000 - Date.now();
  if (wait > 0) await sleep(wait);
  lastCodeSearch = Date.now();
}

async function mcp(ws, tool, args) {
  const client = await clientFor(ws);
  // Byte gate measure: the compact structuredContent JSON length (text length without it),
  // the payload Claude Code gives the model. OCTOCODE_COMPETITOR_BYTES=text measures the text channel.
  const textBytes = process.env.OCTOCODE_COMPETITOR_BYTES === 'text';
  const e = await client.raw(tool, args, '', { keepRaw: true, bytes: (rawSc, text) => (rawSc && !textBytes ? JSON.stringify(rawSc).length : text.length) });
  return { surface: 'mcp', tool, args, ms: Math.round(e.ms), bytes: e.bytes, raw: e.raw, sc: e.sc, text: e.text, isError: e.isError, transport: e.transport };
}

function cli(ws, tool, args) {
  const root = path.join(ROOT, ws);
  const run = runCli(tool, args, { cwd: root, maxBuffer: 256 << 20, timeout: 180_000, env: surfaceEnv(ws, root) });
  let sc;
  try { sc = JSON.parse(run.stdout); } catch { sc = undefined; }
  return { surface: 'cli', tool, args, ms: Math.round(run.ms), bytes: run.stdout.length + run.stderr.length, raw: sc, sc, text: run.stdout + run.stderr, isError: run.status !== 0, transport: run.error?.message };
}

// ---------- recipe templates ----------
const get = (obj, dotted) => dotted.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
/** The latest `next.<name>` or `hints.<name>` continuation offered by the recipe so far. */
const latestHint = (entries, name) => entries.toReversed().flatMap(e => nextHints(e.sc)).find(h => h.path.endsWith(`.${name}`));

function lookup(expr, entries) {
  if (expr === 'T') return REPOS_REL;
  if (expr === 'ROOT') return ROOT;
  const [kind, rest] = [expr.slice(0, expr.indexOf(':')), expr.slice(expr.indexOf(':') + 1)];
  let found;
  if (kind === 'prev') found = get(entries.at(-1)?.sc, rest);
  if (kind === 'hint') {
    const [name, ...tail] = rest.split('.');
    const hint = latestHint(entries, name);
    found = hint && get(hint, tail.join('.'));
  }
  if (found === undefined) throw new Error(`unresolved template {{${expr}}}`);
  return found;
}

/** Fill `{{T}}`, `{{ROOT}}`, `{{prev:path}}` and `{{hint:name.path}}` templates (corpus paths relative to `ws`). */
function resolve(value, entries, ws = '') {
  if (typeof value === 'string') {
    const template = relativize(value, ws, ['{{ROOT}}/{{T}}', '{{T}}']);
    const exact = /^\{\{([^}]+)\}\}$/.exec(template);
    if (exact) return structuredClone(lookup(exact[1], entries));
    return template.replace(/\{\{([^}]+)\}\}/g, (_, expr) => String(lookup(expr, entries)));
  }
  if (Array.isArray(value)) return value.map(v => resolve(v, entries, ws));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, resolve(v, entries, ws)]));
  return value;
}

/**
 * The call brief. Briefs are optional: a research task sends its short
 * `mainGoal`, a single-call lookup or page walk sends none. Legacy runs brief
 * every call with the question.
 */
const taskBrief = task => {
  const mainGoal = NORMALIZED ? task.mainGoal : task.question;
  return mainGoal ? { mainGoal, reasoning: REASONING } : {};
};
/**
 * A continuation as call arguments: its query is the complete input. A step's
 * call-level options (e.g. an explicit responseLength) ride along, except on
 * a response page, whose snapshot binds its own window.
 */
const asCall = (hint, options = {}) => (hint.query.responseSnapshot ? hint.query : { ...hint.query, ...options });

// ---------- recipe steps ----------
/**
 * Walk a step's page continuations, ordered outer → inner: a response reached
 * through an inner continuation offers only that one or deeper (no page ×
 * matchPage fan-out). Page walkers read `next` only (leads live in `hints`).
 * Returns true when every queued continuation was followed.
 */
async function walkPages(ws, step, entries, from) {
  const order = step.walk;
  const seen = new Set();
  const queue = [];
  const enqueue = (entry, minIndex) => {
    for (const h of nextHints(entry.sc).filter(x => /\.next(?:\.|$)/.test(x.path))) {
      const index = order.indexOf(h.path.split('.').at(-1));
      const key = `${h.tool}:${JSON.stringify(h.query)}`;
      if (index >= minIndex && !seen.has(key)) { seen.add(key); queue.push({ hint: h, index }); }
    }
  };
  for (const e of entries.slice(from)) enqueue(e, 0);
  for (let calls = 0; queue.length && calls < (step.max ?? 40); calls += 1) {
    const { hint, index } = queue.shift();
    const e = await mcp(ws, hint.tool, asCall(hint, step.envelope));
    entries.push(e);
    if (e.isError) break;
    enqueue(e, index);
  }
  return queue.length === 0;
}

/** Run one recipe step, appending its responses to `entries`; returns `exhausted` for a walk. */
async function runStep(ws, step, entries, { brief, from }) {
  if (step.tool) {
    const queries = step.queries ? step.queries.map(q => brief(resolve(q, entries, ws))) : [brief(resolve(step.args, entries, ws))];
    entries.push(await mcp(ws, step.tool, { ...step.envelope, queries }));
  } else if (step.cli) {
    entries.push(cli(ws, step.cli, { queries: [brief(resolve(step.args, entries, ws))] }));
  } else if (step.follow) {
    const hint = latestHint(entries, step.follow);
    if (!hint) throw new Error(`no next.${step.follow} or hints.${step.follow} to follow`);
    entries.push(await mcp(ws, hint.tool, asCall(hint, {})));
  } else if (step.walk) return walkPages(ws, step, entries, from);
  return undefined;
}

async function runOctocode(task, ws) {
  const taskBriefFields = taskBrief(task);
  const brief = q => ({ ...taskBriefFields, ...q });
  const entries = [];
  const evidenceSteps = [];
  for (const step of task.octocode) {
    const before = entries.length;
    const exhausted = await runStep(ws, step, entries, { brief, from: evidenceSteps.at(-1)?.from ?? 0 });
    evidenceSteps.push({ from: before, to: entries.length, unsearched: !!step.unsearched, walk: !!step.walk, exhausted });
  }
  return { entries, evidenceSteps };
}

const transientEntry = entries => entries.find(e => (e.transport && TRANSIENT.test(e.transport)) || (e.isError && TRANSIENT.test(e.text.slice(0, 400))));

/** Judge one octocode run of the recipe. */
function judge(run, error, truth) {
  const entries = run?.entries ?? [];
  const ev = emptyEvidence();
  for (const s of run?.evidenceSteps ?? []) for (const e of entries.slice(s.from, s.to)) octocodeEvidence(e, ev, s);
  const verdict = error ? { ok: false, fails: [error.message] } : evaluate(truth, ev);
  const out = {
    ...verdict, evidence: evidenceCounts(ev), calls: entries.length, bytes: entries.reduce((a, e) => a + e.bytes, 0), ms: entries.reduce((a, e) => a + e.ms, 0),
    sensors: taskSensors(entries, run?.evidenceSteps ?? []),
    steps: entries.map(stepBytes),
    errors: entries.filter(e => e.isError).map(e => e.text.slice(0, 200)),
  };
  // Responses for the replay sensor; dropped before results are written.
  Object.defineProperty(out, 'entries', { value: entries, enumerable: false });
  return out;
}

/** Run and judge the octocode recipe; a transient server failure restarts the server and retries once. */
export async function octocodeSide(task, truth, ws, log) {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    let run, error;
    try { run = await runOctocode(task, ws); } catch (e) { error = e; }
    const entries = run?.entries ?? [];
    const transient = transientEntry(entries);
    if (transient && attempt === 0) {
      log(`RETRY [competitors] ${task.id}: ${String(transient.transport ?? transient.text).slice(0, 120)}; restarting MCP`);
      await restart(ws);
      continue;
    }
    if (entries.some(e => e.isError && RATE_LIMIT.test(e.text))) return { skipped: 'rate limited' };
    return judge(run, error, truth);
  }
  return undefined;
}

/** Every distinct continuation offered in a recipe's responses, marked when the recipe already ran it. */
function offeredContinuations(entries) {
  const executed = new Set();
  for (const e of entries) {
    executed.add(`${e.tool}:${canon(e.args)}`);
    for (const q of callRows(e.args)) executed.add(`${e.tool}:${canon(q)}`);
  }
  const seen = new Set();
  const offered = [];
  for (const h of entries.flatMap(e => nextHints(e.raw))) {
    const key = `${h.tool}:${canon(h.query)}`;
    if (seen.has(key)) continue;
    seen.add(key);
    offered.push({ name: h.path.split('.').at(-1), hint: { tool: h.tool, query: h.query }, byRecipe: executed.has(key) });
  }
  return offered;
}

/**
 * Verbatim replay (C-3): every continuation offered in a task's responses runs
 * unchanged and must validate. Continuations the recipe already executed
 * count as replayed; up to REPLAY_MAX others (in offer order) are called once.
 */
export async function replayHints(entries, ws) {
  const offered = offeredContinuations(entries);
  // CLI-only tools (ghCloneRepo) never list on MCP; they are counted, not replayed.
  const client = await clientFor(ws);
  const onMcp = o => client.tools.some(t => t.name === o.hint.tool);
  const sample = offered.filter(o => !o.byRecipe && onMcp(o)).slice(0, Math.max(0, REPLAY_MAX));
  const replays = [];
  for (const o of sample) {
    if (o.hint.tool === 'ghSearchCode') await paceCodeSearch();
    const e = await mcp(ws, o.hint.tool, asCall(o.hint, {}));
    const schema = schemaErrors(e);
    replays.push({ name: o.name, tool: o.hint.tool, surface: e.surface, ok: schema.count === 0, schemaErrors: schema.count, codes: schema.codes, isError: e.isError, rateLimited: e.isError && RATE_LIMIT.test(e.text), bytes: e.bytes, ms: e.ms, detail: schema.count ? schema.detail : undefined });
  }
  return { offered: offered.length, byRecipe: offered.filter(o => o.byRecipe).length, cliOnly: offered.filter(o => !o.byRecipe && !onMcp(o)).length, replayed: replays.length, failed: replays.filter(r => !r.ok).length, replays };
}
