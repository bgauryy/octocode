// Execute canonical tool inputs and retain the measured payload for tokenization.
import path from 'node:path';
import { REPOS_REL, ROOT } from '../paths.mjs';
import { runCli, startServer } from '../mcp-client.mjs';
import { nextHints } from '../responses.mjs';
import { schemaErrors } from '../sensors.mjs';
import { sleep } from '../util.mjs';
import { octocodeEvidence } from './octocode-evidence.mjs';
import { taskSensors, stepBytes } from './response-sensors.mjs';
import { emptyEvidence, evaluate, evidenceCounts } from './truth.mjs';
import { RATE_LIMIT, relativize } from './workspace.mjs';
import { callIssue, callKey, offeredContinuations, walkContinuations } from './navigation.mjs';

export const REPLAY_MAX = Number(process.env.OCTOCODE_COMPETITOR_REPLAY_MAX ?? 3);
const surfaceEnv = root => ({ OCTOCODE_BETA: 'true', WORKSPACE_ROOT: root });
const clients = new Map();
export async function clientFor(ws) {
  if (!clients.has(ws)) {
    const root = path.join(ROOT, ws);
    clients.set(ws, await startServer({ cwd: root, env: surfaceEnv(root) }));
  }
  return clients.get(ws);
}
export function closeClients() {
  for (const client of clients.values()) client.close();
  clients.clear();
}

let lastCodeSearch = 0;
export async function paceCodeSearch() {
  const wait = lastCodeSearch + 7000 - Date.now();
  if (wait > 0) await sleep(wait);
  lastCodeSearch = Date.now();
}

async function mcp(ws, tool, input) {
  const client = await clientFor(ws);
  const textMode = process.env.OCTOCODE_COMPETITOR_BYTES === 'text';
  const payload = (sc, text) => sc && !textMode ? JSON.stringify(sc) : text;
  const entry = await client.raw(tool, input, '', { keepRaw: true, bytes: (sc, text) => Buffer.byteLength(payload(sc, text), 'utf8') });
  return { ...entry, surface: 'mcp', output: payload(entry.raw, entry.text), ms: Math.round(entry.ms) };
}

function cli(ws, tool, input) {
  const root = path.join(ROOT, ws);
  const run = runCli(tool, input, { cwd: root, maxBuffer: 256 << 20, timeout: 180_000, env: surfaceEnv(root) });
  let raw;
  try { raw = JSON.parse(run.stdout); } catch { /* Invalid JSON remains a failed call. */ }
  const empty = run.status === 1 && raw?.results?.length > 0 && raw.results.every(row => ['empty', 'outOfRange'].includes(row.status));
  const output = run.stdout + run.stderr;
  return { surface: 'cli', tool, args: input, ms: Math.round(run.ms), bytes: Buffer.byteLength(output, 'utf8'), output, raw, sc: raw, text: output, isError: !!run.error || !raw || (run.status !== 0 && !empty), rowErrors: raw?.results?.filter(row => row.status === 'error').length ?? 0, transport: run.error?.message };
}

const get = (obj, dotted) => dotted.split('.').reduce((value, key) => value?.[key], obj);
function relatedCall(entries, tool, operation) {
  for (const entry of entries.toReversed()) {
    const calls = nextHints(entry.raw ?? entry.sc).filter(call => call.kind === 'related' && call.tool === tool && call.input.queries?.every(query => query.operation === operation));
    const unique = [...new Map(calls.map(call => [callKey(call), call])).values()];
    if (unique.length > 1) throw new Error(`ambiguous related call ${tool}.${operation}`);
    if (unique.length === 1) return unique[0];
  }
  return undefined;
}
function lookup(expr, entries) {
  if (expr === 'T') return REPOS_REL;
  if (expr === 'ROOT') return ROOT;
  const [kind, rest] = [expr.slice(0, expr.indexOf(':')), expr.slice(expr.indexOf(':') + 1)];
  let found;
  if (kind === 'prev') found = get(entries.at(-1)?.sc, rest);
  if (kind === 'related') {
    const [tool, operation, ...tail] = rest.split('.');
    found = get(relatedCall(entries, tool, operation), tail.join('.'));
  }
  if (found === undefined) throw new Error(`unresolved template {{${expr}}}`);
  return found;
}

function resolve(value, entries, ws) {
  if (typeof value === 'string') {
    const template = relativize(value, ws, ['{{ROOT}}/{{T}}', '{{T}}']);
    const exact = /^\{\{([^}]+)\}\}$/.exec(template);
    if (exact) return structuredClone(lookup(exact[1], entries));
    return template.replace(/\{\{([^}]+)\}\}/g, (_, expr) => String(lookup(expr, entries)));
  }
  if (Array.isArray(value)) return value.map(item => resolve(item, entries, ws));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, resolve(child, entries, ws)]));
  return value;
}

function recipeInput(step, entries, ws) {
  if (!step.input || ['args', 'queries', 'envelope'].some(key => key in step)) throw new Error('recipe requires one canonical input envelope');
  return resolve(step.input, entries, ws);
}

async function runStep(step, ws, run, { invoke, invokeCli }) {
  if (step.tool || step.cli) {
    const input = recipeInput(step, run.entries, ws);
    run.entries.push(await (step.cli ? invokeCli : invoke)(step.tool ?? step.cli, input));
    return undefined;
  }
  if (step.walk !== true) throw new Error('recipe step requires tool, cli, or walk:true');
  const walked = await walkContinuations(run.entries, run.evidenceSteps.at(-1)?.from ?? 0, invoke, step.max ?? 40);
  if (!walked.exhausted) throw new Error(walked.error ?? `continuation budget exhausted with ${walked.remaining} pages pending`);
  return true;
}

/** Failed steps remain in the evidence and cost denominator. */
export async function runRecipe(task, ws, invoke, invokeCli = invoke) {
  const run = { entries: [], evidenceSteps: [], error: null };
  for (const step of task.octocode) {
    const before = run.entries.length;
    let exhausted;
    try {
      exhausted = await runStep(step, ws, run, { invoke, invokeCli });
      const failed = run.entries.slice(before).map(callIssue).find(Boolean);
      if (failed) throw new Error(failed);
    } catch (error) { run.error = error.message; }
    run.evidenceSteps.push({ from: before, to: run.entries.length, unsearched: !!step.unsearched, walk: step.walk === true, exhausted });
    if (run.error) break;
  }
  return run;
}

function judge(run, truth) {
  const ev = emptyEvidence();
  for (const step of run.evidenceSteps) for (const entry of run.entries.slice(step.from, step.to)) octocodeEvidence(entry, ev, step);
  const verdict = evaluate(truth, ev);
  const fails = [...verdict.fails, ...(run.error ? [run.error] : [])];
  const out = {
    ok: fails.length === 0, fails, evidence: evidenceCounts(ev), calls: run.entries.length,
    bytes: run.entries.reduce((sum, entry) => sum + entry.bytes, 0), ms: run.entries.reduce((sum, entry) => sum + entry.ms, 0),
    sensors: taskSensors(run.entries, run.evidenceSteps), steps: run.entries.map(stepBytes),
    errors: run.entries.map(callIssue).filter(Boolean),
  };
  Object.defineProperty(out, 'entries', { value: run.entries, enumerable: false });
  return out;
}

export async function octocodeSide(task, truth, ws) {
  const run = await runRecipe(task, ws, (tool, input) => mcp(ws, tool, input), (tool, input) => cli(ws, tool, input));
  return judge(run, truth);
}

export async function replayCalls(entries, invoke, max = REPLAY_MAX) {
  const offered = offeredContinuations(entries);
  const replays = [];
  for (const offer of offered.filter(item => !item.byRecipe).slice(0, Math.max(0, max))) {
    const entry = await invoke(offer.call.tool, offer.call.input);
    const schema = schemaErrors(entry);
    const error = callIssue(entry);
    replays.push({ name: offer.name, ...stepBytes(entry), ok: !error && schema.count === 0, schemaErrors: schema.count, codes: schema.codes, rateLimited: !!error && RATE_LIMIT.test(entry.text ?? ''), detail: error ?? (schema.count ? schema.detail : undefined) });
  }
  return { offered: offered.length, byRecipe: offered.filter(item => item.byRecipe).length, cliOnly: replays.filter(entry => entry.surface === 'cli').length, replayed: replays.length, failed: replays.filter(entry => !entry.ok).length, replays };
}

export async function replayHints(entries, ws) {
  const client = await clientFor(ws);
  return replayCalls(entries, async (tool, input) => {
    if (tool === 'ghSearchCode') await paceCodeSearch();
    return client.tools.some(item => item.name === tool) ? mcp(ws, tool, input) : cli(ws, tool, input);
  });
}
