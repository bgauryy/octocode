#!/usr/bin/env node
// Tally agent transcripts per task: calls, failed calls, characters, tokens (four fields, never summed) and wall
// time; with --before, each task's change and whether it exceeds the run-to-run spread. Reads the record shapes
// octocode-context-audit's usage.mjs reads (assistant tool_use, user tool_result), plus usage, timestamps and the
// stream-json result record.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Usage: node harness-tally.mjs <transcript.jsonl>... [--task <regex>] [--before <tally.json>]

Inputs: Claude Code session or subagent transcripts (~/.claude/projects/<project>/<session>.jsonl,
<session>/subagents/agent-*.jsonl) or \`claude -p --output-format stream-json --verbose\` captures.
Each file is one run. Runs of one task are grouped by --task, a regex matched against the path
as given (first capture group, else the whole match); default: the parent folder for a file named
transcript.jsonl, else the file name without .jsonl.

Per run: calls, failed (host is_error, or an Octocode JSON reply with errorCode or a results[] row
with status "error"), retries (calls right after a failed call to the same tool), requests (model
requests), chars {calls, results, answer, total} in Unicode code points, tokens {input, cacheRead,
cacheWrite, output} (the stream-json result record when present, else one usage per message id),
firstCacheWrite (the first request's cache write: 0 means it re-read an earlier run's cache),
wallMs (result duration_ms, else first-to-last timestamp), costUsd (stream-json only, else null).
Per task: runs, median, min, max and spread (max - min) of each metric.
--before <tally.json>: a saved earlier tally; adds comparison.tasks[] with before, after, delta and
spread (the larger of the two spreads) per metric; beyondSpread is |delta| > spread, or null when
either side has fewer than 2 runs.

Effects: reads files only. Output: JSON on stdout. Exit 0 ok; 2 invalid input
(missing or unreadable file, no assistant records, bad --task or --before); 1 unexpected failure.`;

/** The compared metrics, as dotted paths into a run record. */
export const METRICS = [
  'calls', 'failed', 'retries', 'requests',
  'chars.calls', 'chars.results', 'chars.answer', 'chars.total',
  'tokens.input', 'tokens.cacheRead', 'tokens.cacheWrite', 'tokens.output',
  'firstCacheWrite', 'wallMs',
];

const codePoints = (text) => {
  let count = 0;
  for (const _ of text) count += 1;
  return count;
};
const read = (record, dotted) => dotted.split('.').reduce((value, key) => (value == null ? value : value[key]), record);
const bump = (map, key) => { map[key] = (map[key] ?? 0) + 1; };

function resultText(content) {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.map((part) => (part && part.type === 'text' && typeof part.text === 'string' ? part.text : '')).join('');
}

/** An Octocode JSON reply that reports an error in-band (MCP leaves isError unset for per-row errors). */
export function inBandError(text) {
  if (!text.startsWith('{')) return false;
  try {
    const reply = JSON.parse(text);
    return typeof reply?.errorCode === 'string' || (Array.isArray(reply?.results) && reply.results.some((row) => row?.status === 'error'));
  } catch {
    return false;
  }
}

function usageTokens(usage) {
  return {
    input: usage?.input_tokens ?? 0,
    cacheRead: usage?.cache_read_input_tokens ?? 0,
    cacheWrite: usage?.cache_creation_input_tokens ?? 0,
    output: usage?.output_tokens ?? 0,
  };
}

function parseLines(text) {
  const records = [];
  let malformedLines = 0;
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    try {
      const record = JSON.parse(line);
      if (record && typeof record === 'object' && !Array.isArray(record)) records.push(record);
      else malformedLines += 1;
    } catch {
      malformedLines += 1;
    }
  }
  return { records, malformedLines };
}

/** One transcript's numbers; null when it holds no assistant record. */
export function tallyTranscript(text) {
  const { records, malformedLines } = parseLines(text);
  const messages = new Map(); // message id -> { usage, text }
  const calls = new Map(); // tool_use id -> { tool, failed }
  const chars = { calls: 0, results: 0, answer: 0, total: 0 };
  let result = null;
  let first = Infinity;
  let last = -Infinity;
  for (const record of records) {
    const at = Date.parse(record.timestamp);
    if (Number.isFinite(at)) { first = Math.min(first, at); last = Math.max(last, at); }
    if (record.type === 'result') result = record;
    const message = record.message;
    if ((record.type !== 'assistant' && record.type !== 'user') || !message || typeof message !== 'object') continue;
    const content = Array.isArray(message.content) ? message.content : [];
    if (record.type === 'assistant') {
      if (message.model === '<synthetic>') continue;
      const id = message.id ?? record.uuid;
      const entry = messages.get(id) ?? { usage: null, text: '' };
      if (message.usage) entry.usage = message.usage;
      for (const part of content) {
        if (part?.type === 'text' && typeof part.text === 'string') entry.text += part.text;
        if (part?.type === 'tool_use' && typeof part.id === 'string' && !calls.has(part.id)) {
          calls.set(part.id, { tool: String(part.name), failed: false });
          chars.calls += codePoints(JSON.stringify(part.input ?? {}));
        }
      }
      messages.set(id, entry);
    } else {
      for (const part of content) {
        if (part?.type !== 'tool_result') continue;
        const body = resultText(part.content);
        chars.results += codePoints(body);
        const call = calls.get(part.tool_use_id);
        if (call && (part.is_error === true || inBandError(body))) call.failed = true;
      }
    }
  }
  if (messages.size === 0) return null;
  const ordered = [...messages.values()];
  const answer = typeof result?.result === 'string' ? result.result : (ordered.findLast((entry) => entry.text)?.text ?? '');
  chars.answer = codePoints(answer);
  chars.total = chars.calls + chars.results + chars.answer;
  const fromMessages = ordered.reduce((sum, entry) => {
    const tokens = usageTokens(entry.usage);
    for (const key of Object.keys(sum)) sum[key] += tokens[key];
    return sum;
  }, { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 });
  const tools = {};
  const failedTools = {};
  let failed = 0;
  let retries = 0;
  const lastFailed = new Map();
  for (const call of calls.values()) {
    bump(tools, call.tool);
    if (lastFailed.get(call.tool)) retries += 1;
    lastFailed.set(call.tool, call.failed);
    if (call.failed) { failed += 1; bump(failedTools, call.tool); }
  }
  const resultWall = Number.isFinite(result?.duration_ms) ? result.duration_ms : null;
  const spanWall = Number.isFinite(first) ? last - first : null;
  return {
    format: result || records.some((record) => record.type === 'system' && record.subtype === 'init') ? 'stream-json' : 'session',
    calls: calls.size,
    failed,
    retries,
    requests: messages.size,
    tools,
    failedTools,
    chars,
    tokens: { ...(result?.usage ? usageTokens(result.usage) : fromMessages), source: result?.usage ? 'result' : 'messages' },
    firstCacheWrite: usageTokens(ordered.find((entry) => entry.usage)?.usage).cacheWrite,
    wallMs: resultWall ?? spanWall,
    wallSource: resultWall != null ? 'result.duration_ms' : spanWall != null ? 'timestamps' : null,
    costUsd: Number.isFinite(result?.total_cost_usd) ? result.total_cost_usd : null,
    malformedLines,
  };
}

/** The task a transcript belongs to. */
export function taskOf(file, pattern) {
  if (pattern) {
    const match = pattern.exec(file);
    return match ? (match[1] ?? match[0]) : null;
  }
  const base = path.basename(file);
  return base === 'transcript.jsonl' ? path.basename(path.dirname(path.resolve(file))) : base.replace(/\.jsonl$/, '');
}

const median = (values) => {
  const sorted = values.toSorted((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

/** Runs grouped by task: median, min, max and spread of every metric. */
export function summarize(runs) {
  const groups = new Map();
  for (const run of runs) groups.set(run.task, [...(groups.get(run.task) ?? []), run]);
  return [...groups].map(([task, group]) => {
    const summary = { task, runs: group.length, median: {}, min: {}, max: {}, spread: {} };
    for (const metric of METRICS) {
      const values = group.map((run) => read(run, metric)).filter(Number.isFinite);
      summary.median[metric] = values.length ? median(values) : null;
      summary.min[metric] = values.length ? Math.min(...values) : null;
      summary.max[metric] = values.length ? Math.max(...values) : null;
      summary.spread[metric] = values.length ? Math.max(...values) - Math.min(...values) : null;
    }
    return summary;
  });
}

/** Before → after per task and metric, judged against the run-to-run spread. */
export function compare(beforeTasks, afterTasks) {
  const before = new Map(beforeTasks.map((task) => [task.task, task]));
  const after = new Map(afterTasks.map((task) => [task.task, task]));
  const tasks = [];
  for (const [name, now] of after) {
    const then = before.get(name);
    if (!then) continue;
    const metrics = {};
    for (const metric of METRICS) {
      const b = then.median?.[metric];
      const a = now.median?.[metric];
      if (!Number.isFinite(b) || !Number.isFinite(a)) { metrics[metric] = { before: b ?? null, after: a ?? null, delta: null, spread: null, beyondSpread: null }; continue; }
      const spread = Math.max(then.spread?.[metric] ?? 0, now.spread?.[metric] ?? 0);
      const delta = a - b;
      metrics[metric] = { before: b, after: a, delta, spread, beyondSpread: then.runs >= 2 && now.runs >= 2 ? Math.abs(delta) > spread : null };
    }
    tasks.push({ task: name, runs: { before: then.runs, after: now.runs }, metrics });
  }
  return {
    tasks,
    onlyBefore: [...before.keys()].filter((name) => !after.has(name)),
    onlyAfter: [...after.keys()].filter((name) => !before.has(name)),
  };
}

function parseArgs(argv) {
  const files = [];
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--task' || arg === '--before') {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) fail('invalidInput', `${arg} requires a value`, 2);
      options[arg.slice(2)] = value;
      index += 1;
    } else if (arg.startsWith('--')) fail('invalidInput', `unknown option ${arg}; see --help`, 2);
    else files.push(arg);
  }
  return { files, options };
}

function main(argv) {
  const { files, options } = parseArgs(argv);
  if (options.help) { console.log(HELP); return; }
  if (!files.length) fail('invalidInput', 'pass at least one transcript.jsonl; see --help', 2);
  let pattern = null;
  if (options.task) {
    try { pattern = new RegExp(options.task); } catch (error) { fail('invalidTask', `--task is not a valid regex: ${errorMessage(error)}`, 2); }
  }
  let beforeTasks = null;
  if (options.before) {
    try {
      beforeTasks = JSON.parse(readFileSync(options.before, 'utf8')).tasks;
    } catch (error) {
      fail('invalidBefore', `cannot read --before ${options.before}: ${errorMessage(error)}`, 2);
    }
    if (!Array.isArray(beforeTasks) || beforeTasks.some((task) => typeof task?.task !== 'string' || !task.median)) {
      fail('invalidBefore', `--before ${options.before} is not a harness-tally output (tasks[] with task and median)`, 2);
    }
  }
  const runs = files.map((file) => {
    let text;
    try { text = readFileSync(file, 'utf8'); } catch (error) { fail('unreadableTranscript', `${file}: ${errorMessage(error)}`, 2); }
    const tally = tallyTranscript(text);
    if (!tally) fail('notATranscript', `${file} has no assistant records`, 2);
    const task = taskOf(file, pattern);
    if (task == null) fail('invalidTask', `--task ${options.task} does not match ${file}`, 2);
    return { file, task, ...tally };
  });
  const tasks = summarize(runs);
  const output = { ok: true, runs, tasks };
  if (beforeTasks) output.comparison = compare(beforeTasks, tasks);
  console.log(JSON.stringify(output, null, 2));
}

if (isMainModule(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    fail('tallyFailed', errorMessage(error), 1);
  }
}
