#!/usr/bin/env node
// Numbers for a stored round: per-question metrics and scores, paired ratios with a bootstrap interval, a
// ledger of what the replies were made of, and the measured price of a model call.
import fs from 'node:fs';
import path from 'node:path';
import { HarnessError, cli, codePoints, isMain, isRecord, loadManifest, parseArgs, passPath, questions, readTranscript, runPath, seeded } from './lib.mjs';

const HELP = `Usage: node packages/octocode-benchmark/harness/analyze.mjs <round manifest> [--out <dir>] [--json]

Reads every stored run and verdict of the round and writes numbers.json (schemaVersion 2) and tables.md to
<campaign>/analysis/ (or --out). Prints, per group of questions:
  - per arm: answers, quality, calls (failed), reply characters, tokens and cost;
  - per "compare" pair of the manifest: the per-question ratio of cost, new input, cached re-reads, output, model
    calls, tool calls and reply characters as a geometric mean with a 95% bootstrap interval (questions
    resampled 10,000 times by a generator seeded from the manifest), and on how many questions each side is lower;
  - the ledger per arm: reply characters and new-input tokens by tool and by output field path, file requests by
    mode, cursor calls, failures by cause (refusals apart from errors), first-call cached tokens;
  - the price of one more model call, from the round's own usage and cost.
Tokens per reply are the provider's: a model request's new input is shared between the replies it read (and the
model's own previous turn) by character count. No tokenizer runs.

  --out <dir>   write numbers.json and tables.md there
  --json        print numbers.json instead of the text summary
Exit codes: 0 analyzed; 2 bad input or no stored run.`;

export const SCHEMA_VERSION = 2;
export const RESAMPLES = 10_000;
/** The metrics a paired ratio is reported for: [key in a run's metrics, label]. */
export const RATIO_METRICS = [
  ['cost', 'cost'], ['new', 'new input'], ['cached', 'cached re-reads'], ['output', 'output'], ['modelCalls', 'model calls'],
  ['calls', 'tool calls'], ['chars', 'reply characters'],
];

const sum = values => values.reduce((total, value) => total + value, 0);
const mean = values => (values.length ? sum(values) / values.length : undefined);
export function median(values) {
  if (!values.length) return 0;
  const sorted = values.toSorted((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
const geomean = values => Math.exp(sum(values.map(Math.log)) / values.length);
const round = (value, digits = 2) => (value === undefined || !Number.isFinite(value) ? undefined : Number(value.toFixed(digits)));
const bump = (table, key, amounts) => {
  const row = (table[key] ??= {});
  for (const [name, amount] of Object.entries(amounts)) row[name] = (row[name] ?? 0) + amount;
};

/**
 * Paired ratio a/b over questions: geometric mean, 95% percentile bootstrap interval (questions resampled),
 * median, range and how often each side is lower. Pairs where either side is not a positive number are left
 * out and counted in `skipped`.
 */
export function pairedRatio(pairs, random, resamples = RESAMPLES) {
  const usable = pairs.filter(([a, b]) => a > 0 && b > 0);
  const out = { n: usable.length, skipped: pairs.length - usable.length, aLower: usable.filter(([a, b]) => a < b).length, bLower: usable.filter(([a, b]) => b < a).length };
  if (!usable.length) return out;
  const logs = usable.map(([a, b]) => Math.log(a / b));
  const means = Array.from({ length: resamples }, () => {
    let total = 0;
    for (let i = 0; i < logs.length; i += 1) total += logs[Math.floor(random() * logs.length)];
    return total / logs.length;
  }).toSorted((a, b) => a - b);
  const ratios = logs.map(Math.exp);
  return {
    ...out,
    geomean: geomean(ratios),
    low: Math.exp(means[Math.floor(0.025 * resamples)]),
    high: Math.exp(means[Math.ceil(0.975 * resamples) - 1]),
    median: median(ratios),
    min: Math.min(...ratios),
    max: Math.max(...ratios),
  };
}

/** Least-squares prices per token kind from each session's usage and reported cost: cost ≈ Σ price × tokens. */
export function fitPrices(rows) {
  const keys = ['input', 'cacheWrite', 'cacheRead', 'output'].filter(key => rows.some(row => row[key] > 0));
  const size = keys.length;
  if (rows.length < size || !size) return undefined;
  // Tokens differ by orders of magnitude between kinds: scale each column to unit mean before solving.
  const scale = keys.map(key => mean(rows.map(row => row[key])));
  const a = Array.from({ length: size }, () => Array.from({ length: size + 1 }, () => 0));
  for (const row of rows) {
    const x = keys.map((key, i) => row[key] / scale[i]);
    for (let i = 0; i < size; i += 1) {
      for (let j = 0; j < size; j += 1) a[i][j] += x[i] * x[j];
      a[i][size] += x[i] * row.cost;
    }
  }
  for (let col = 0; col < size; col += 1) {
    let pivot = col;
    for (let r = col + 1; r < size; r += 1) if (Math.abs(a[r][col]) > Math.abs(a[pivot][col])) pivot = r;
    if (Math.abs(a[pivot][col]) < 1e-12) return undefined;
    [a[col], a[pivot]] = [a[pivot], a[col]];
    for (let r = 0; r < size; r += 1) {
      if (r === col) continue;
      const factor = a[r][col] / a[col][col];
      for (let c = col; c <= size; c += 1) a[r][c] -= factor * a[col][c];
    }
  }
  const perToken = Object.fromEntries(keys.map((key, i) => [key, a[i][size] / a[i][i] / scale[i]]));
  const predicted = row => sum(keys.map(key => perToken[key] * row[key]));
  const worst = Math.max(...rows.map(row => Math.abs(predicted(row) - row.cost) / Math.max(row.cost, 1e-9)));
  return { perMillion: Object.fromEntries(keys.map(key => [key, round(perToken[key] * 1e6, 4)])), perToken, sessions: rows.length, worstRelativeError: round(worst, 4) };
}

/** Whether a reply reports a failure. Shell arms fail by exit, HTTP status or refusal; tool arms by an error row. */
export function replyFailed(kind, result) {
  if (result.isError) return true;
  if (kind === 'shell') return /\(HTTP [45]\d\d\)|read-only benchmark\): refused/.test(result.text);
  let reply;
  try {
    reply = JSON.parse(result.text);
  } catch {
    return /^\s*(result: \d+ \(error\)|status: error)/.test(result.text);
  }
  if (!isRecord(reply)) return false;
  if (!Array.isArray(reply.results)) return 'errorCode' in reply || reply.status === 'error';
  return reply.results.some(row => isRecord(row) && (row.status === 'error' || 'errorCode' in row || (isRecord(row.data) && 'errorCode' in row.data)));
}

/** The cause of a failed reply, in words a table can group by. Refusals start with "refused". */
export function failureCause(kind, result) {
  if (kind === 'shell') {
    if (/read-only benchmark\): refused/.test(result.text)) return 'refused: wrapper (write or non-GET call)';
    const http = /\(HTTP ([45]\d\d)\)/.exec(result.text);
    if (http) return `error: HTTP ${http[1]}`;
    const exit = /^Exit code (\d+)/.exec(result.text);
    if (exit) return `error: exit ${exit[1]}`;
    return 'refused: host permission';
  }
  const code = /"errorCode":"(\w+)"/.exec(result.text)?.[1];
  const errorClass = /"errorClass":"(\w+)"/.exec(result.text)?.[1];
  if (code) return `${errorClass === 'refused' ? 'refused' : 'error'}: ${code}`;
  return result.isError ? 'error: host' : 'error: unnamed';
}

const PLAIN_KEY = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** Characters of a JSON reply by field path (array steps as `*`, data-named keys as `{key}`), plus JSON syntax. */
export function fieldChars(text) {
  const total = codePoints(text);
  let reply;
  try {
    reply = JSON.parse(text);
  } catch {
    return { '(text)': total };
  }
  if (!isRecord(reply) && !Array.isArray(reply)) return { '(text)': total };
  const fields = {};
  let counted = 0;
  const visit = (value, at) => {
    if (Array.isArray(value)) for (const item of value) visit(item, `${at}/*`);
    else if (isRecord(value)) for (const [key, item] of Object.entries(value)) visit(item, `${at}/${PLAIN_KEY.test(key) ? key : '{key}'}`);
    else {
      const chars = codePoints(JSON.stringify(value));
      fields[at.slice(1)] = (fields[at.slice(1)] ?? 0) + chars;
      counted += chars;
    }
  };
  visit(reply, '');
  fields['(json syntax and keys)'] = total - counted;
  return fields;
}

/** Splits a shell command at unquoted pipes; returns each stage's words (quotes kept out of splitting only). */
export function pipeline(command) {
  const stages = [];
  let current = '';
  let quote;
  for (let i = 0; i < command.length; i += 1) {
    const char = command[i];
    if (quote) {
      if (char === '\\' && quote === '"') current += char + (command[++i] ?? '');
      else {
        if (char === quote) quote = undefined;
        current += char;
      }
    } else if (char === '"' || char === "'") {
      quote = char;
      current += char;
    } else if (char === '|' && command[i + 1] !== '|' && command[i - 1] !== '|') {
      stages.push(current);
      current = '';
    } else current += char;
  }
  stages.push(current);
  return stages.map(stage => stage.trim().split(/\s+/).filter(Boolean));
}

const FLAGS_WITH_VALUE = new Set(['-X', '--method', '-H', '--header', '-q', '--jq', '-f', '-F', '--field', '--raw-field', '-R', '--repo', '-t', '--template', '--json', '-L', '--limit']);

/** A shell call as {tool, filters, fileMode}: the gh command by kind, the filters it is piped through. */
export function shellCall(command) {
  const [first = [], ...rest] = pipeline(command);
  // Words that are neither a flag nor the value of a flag that takes one.
  const words = first.filter((word, i) => !word.startsWith('-') && !FLAGS_WITH_VALUE.has(first[i - 1]));
  let tool = words.slice(0, 3).join(' ') || '(empty)';
  let endpoint = '';
  if (words[0] === 'gh' && words[1] === 'api') {
    endpoint = (words[2] ?? '').replace(/^['"]|['"]$/g, '').replace(/^\//, '').split('?')[0];
    const parts = endpoint.split('/');
    tool = `gh api ${parts[0] === 'repos' ? ['repos/*', ...parts.slice(3, parts[3] === 'git' ? 5 : 4)].join('/') : parts.slice(0, 2).join('/')}`;
  } else if (words[0] !== 'gh') tool = '(not a gh command)';
  const filters = rest.map(stage => stage[0]).filter(Boolean);
  let fileMode;
  if (/^repos\/[^/]+\/[^/]+\/contents\//.test(endpoint)) {
    if (filters.some(filter => ['grep', 'rg'].includes(filter))) fileMode = 'filter (grep)';
    else if (filters.some(filter => ['sed', 'head', 'tail'].includes(filter))) fileMode = 'range (sed, head, tail)';
    else fileMode = 'whole';
  }
  return { tool, filters: filters.join(' | ') || '(none)', fileMode };
}

const READ_TOOL = /fetch|filecontent/i;

/** How one query of a file-read tool selects lines. */
export function readMode(query) {
  if (!isRecord(query)) return 'whole';
  const block = query.block === true ? ' + block' : '';
  if (typeof query.view === 'string' && query.view !== 'content') return `view: ${query.view}`;
  if (Array.isArray(query.ranges) && query.matchString !== undefined) return `matchString in ranges${block}`;
  if (Array.isArray(query.ranges) || query.startLine !== undefined || query.endLine !== undefined) return `ranges${block}`;
  if (query.matchString !== undefined) return `matchString${block}`;
  return 'whole';
}

const rangeLines = query => sum((Array.isArray(query?.ranges) ? query.ranges : []).map(range => (Number.isFinite(range?.startLine) && Number.isFinite(range?.endLine) ? Math.max(0, range.endLine - range.startLine + 1) : 0)));

/** Everything measured from one stored run. */
export function runMetrics(dir, kind) {
  const stored = JSON.parse(fs.readFileSync(path.join(dir, 'result.json'), 'utf8'));
  const result = stored.result ?? {};
  const { tools, steps } = readTranscript(path.join(dir, 'transcript.jsonl'));
  const usage = result.usage ?? {};
  const ledger = { tools: {}, fields: {}, filters: {}, fileReads: {}, rangeLines: 0, cursor: { calls: 0, chars: 0 }, failures: {}, tokens: { prompt: 0, modelTurn: 0, replies: 0 } };
  let calls = 0;
  let failed = 0;
  let chars = 0;
  let callChars = 0;
  let waits = 0;
  const stepsOut = [];
  steps.forEach((step, at) => {
    const uses = new Map(step.blocks.filter(block => block.type === 'tool_use').map(block => [block.id, block]));
    calls += uses.size;
    callChars += sum([...uses.values()].map(block => block.chars));
    const newTokens = (step.usage.input_tokens ?? 0) + (step.usage.cache_creation_input_tokens ?? 0);
    stepsOut.push({ new: newTokens, cacheRead: step.usage.cache_read_input_tokens ?? 0, input: step.usage.input_tokens ?? 0, cacheWrite: step.usage.cache_creation_input_tokens ?? 0 });
    // The next request's new input is this step's own blocks plus the replies it received.
    const nextNew = steps[at + 1] ? (steps[at + 1].usage.input_tokens ?? 0) + (steps[at + 1].usage.cache_creation_input_tokens ?? 0) : 0;
    const replyChars = step.results.map(reply => codePoints(reply.text));
    const turnChars = sum(step.blocks.map(block => block.chars));
    const shared = turnChars + sum(replyChars);
    const perChar = shared ? nextNew / shared : 0;
    if (at === 0) ledger.tokens.prompt += newTokens;
    ledger.tokens.modelTurn += turnChars * perChar;
    step.results.forEach((reply, i) => {
      const use = uses.get(reply.id) ?? step.blocks.find(block => block.id === reply.id);
      const count = replyChars[i];
      const tokens = count * perChar;
      chars += count;
      ledger.tokens.replies += tokens;
      for (const wait of reply.text.matchAll(/"kind":"\w+","ms":(\d+)/g)) waits += Number(wait[1]) / 1000;
      const isFailed = replyFailed(kind, reply);
      if (isFailed) {
        failed += 1;
        bump(ledger.failures, failureCause(kind, reply), { calls: 1 });
      }
      let tool = use?.name?.split('__').at(-1) ?? '(unknown)';
      if (kind === 'shell') {
        const call = shellCall(String(use?.input?.command ?? ''));
        tool = call.tool;
        bump(ledger.filters, call.filters, { calls: 1, chars: count, tokens });
        if (call.fileMode) bump(ledger.fileReads, call.fileMode, { requests: 1 });
        bump(ledger.fields, '(output)', { chars: count, tokens });
      } else {
        const fields = fieldChars(reply.text);
        for (const [field, amount] of Object.entries(fields)) bump(ledger.fields, field, { chars: amount, tokens: amount * perChar });
        const input = use?.input;
        if (isRecord(input) && input.cursor !== undefined) {
          ledger.cursor.calls += 1;
          ledger.cursor.chars += count;
        } else if (READ_TOOL.test(tool)) {
          for (const query of Array.isArray(input?.queries) ? input.queries : [input]) {
            bump(ledger.fileReads, readMode(query), { requests: 1 });
            ledger.rangeLines += rangeLines(query);
          }
        }
      }
      bump(ledger.tools, tool, { calls: 1, failed: isFailed ? 1 : 0, chars: count, tokens });
    });
  });
  const first = stepsOut[0] ?? { cacheRead: 0, cacheWrite: 0 };
  const newInput = (usage.input_tokens ?? 0) + (usage.cache_creation_input_tokens ?? 0);
  const answerChars = codePoints(fs.readFileSync(path.join(dir, 'answer.md'), 'utf8'));
  return {
    rc: stored.rc,
    time: stored.wall_s,
    turns: result.num_turns,
    calls,
    failed,
    chars,
    charsAll: chars + callChars + answerChars,
    new: newInput,
    cached: usage.cache_read_input_tokens ?? 0,
    input: newInput + (usage.cache_read_input_tokens ?? 0),
    output: usage.output_tokens ?? 0,
    cost: result.total_cost_usd ?? 0,
    firstCallCacheRead: first.cacheRead,
    firstCallCacheWrite: first.cacheWrite,
    modelCalls: stepsOut.length,
    answerChars,
    nTools: (tools ?? []).length,
    waits,
    denials: (result.permission_denials ?? []).length,
    usage: { input: usage.input_tokens ?? 0, cacheWrite: usage.cache_creation_input_tokens ?? 0, cacheRead: usage.cache_read_input_tokens ?? 0, output: usage.output_tokens ?? 0 },
    steps: stepsOut,
    ledger,
  };
}

/** The verdicts of one question in one pass: scores (the mean of both gradings when regraded), facts, and who won. */
export function judged(dir, name, arms) {
  const read = file => (fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : undefined);
  const first = read(path.join(dir, name, 'verdict.json'));
  if (!first) return undefined;
  const swap = read(path.join(dir, `${name}-swap`, 'verdict.json'));
  const scored = arms.filter(arm => first.scores[arm] !== undefined);
  const out = {
    first: first.scores,
    swap: swap?.scores ?? null,
    score: Object.fromEntries(scored.map(arm => [arm, swap ? (first.scores[arm] + swap.scores[arm]) / 2 : first.scores[arm]])),
    facts: first.facts ?? null,
    vs: {},
  };
  const one = (scores, a, b) => (scores[a] - scores[b] > 1 ? 'won' : scores[b] - scores[a] > 1 ? 'lost' : 'tied');
  for (const a of scored) {
    for (const b of scored) {
      if (a === b) continue;
      const before = one(first.scores, a, b);
      out.vs[`${a}>${b}`] = !swap || before === one(swap.scores, a, b) ? before : 'unresolved';
    }
  }
  return out;
}

function mergeLedger(into, from) {
  for (const table of ['tools', 'fields', 'filters', 'fileReads', 'failures']) for (const [key, amounts] of Object.entries(from[table])) bump(into[table], key, amounts);
  into.rangeLines += from.rangeLines;
  into.cursor.calls += from.cursor.calls;
  into.cursor.chars += from.cursor.chars;
  for (const key of Object.keys(into.tokens)) into.tokens[key] += from.tokens[key];
}

const emptyLedger = () => ({ tools: {}, fields: {}, filters: {}, fileReads: {}, rangeLines: 0, cursor: { calls: 0, chars: 0 }, failures: {}, tokens: { prompt: 0, modelTurn: 0, replies: 0 } });

/** One arm over a list of {metrics, judge} rows (one per question and pass). */
function summarize(arm, rows, others, prices) {
  const runs = rows.map(row => row.metrics);
  const scores = rows.map(row => row.judge?.score?.[arm]).filter(score => score !== undefined);
  const ledger = emptyLedger();
  for (const run of runs) mergeLedger(ledger, run.ledger);
  const later = runs.flatMap(run => run.steps.slice(1));
  const out = {
    n: runs.length,
    judged: scores.length,
    quality: round(mean(scores)) ?? null,
    tens: scores.filter(score => score === 10).length,
    eightPlus: scores.filter(score => score >= 8).length,
    calls: sum(runs.map(run => run.calls)),
    failed: sum(runs.map(run => run.failed)),
    modelCalls: sum(runs.map(run => run.modelCalls)),
    time: median(runs.map(run => run.time)),
    timeTotal: Math.round(sum(runs.map(run => run.time))),
    input: median(runs.map(run => run.input)),
    new: median(runs.map(run => run.new)),
    cached: median(runs.map(run => run.cached)),
    output: median(runs.map(run => run.output)),
    chars: median(runs.map(run => run.chars)),
    charsTotal: sum(runs.map(run => run.chars)),
    charsAllTotal: sum(runs.map(run => run.charsAll)),
    cost: round(sum(runs.map(run => run.cost))),
    newTotal: sum(runs.map(run => run.new)),
    cachedTotal: sum(runs.map(run => run.cached)),
    outputTotal: sum(runs.map(run => run.output)),
    coldPrompt: runs.filter(run => run.firstCallCacheRead === 0).length,
    firstCallCacheRead: median(runs.map(run => run.firstCallCacheRead)),
    firstCallCacheWrite: median(runs.map(run => run.firstCallCacheWrite)),
    cacheShare: round(sum(runs.map(run => run.cached)) / Math.max(1, sum(runs.map(run => run.input))), 3),
    rcBad: runs.filter(run => run.rc !== 0).length,
    waits: Math.round(sum(runs.map(run => run.waits))),
  };
  if (prices) {
    const { perToken } = prices;
    const price = key => perToken[key] ?? 0;
    out.costSplit = {
      newInput: round(sum(runs.map(run => run.usage.input * price('input') + run.usage.cacheWrite * price('cacheWrite')))),
      cachedReread: round(sum(runs.map(run => run.usage.cacheRead * price('cacheRead')))),
      output: round(sum(runs.map(run => run.usage.output * price('output')))),
    };
    // One more model call re-reads the context so far from cache and writes output; what it then reads as new
    // input (the reply it asked for) is priced beside it, since a call can be added without adding a reply.
    const outputPerCall = out.modelCalls ? (out.outputTotal * price('output')) / out.modelCalls : 0;
    const cachedPerCall = mean(later.map(step => step.cacheRead * price('cacheRead'))) ?? 0;
    const newPerCall = mean(later.map(step => step.input * price('input') + step.cacheWrite * price('cacheWrite'))) ?? 0;
    out.extraModelCall = {
      laterCalls: later.length,
      cachedReread: round(cachedPerCall, 4),
      output: round(outputPerCall, 4),
      withoutNewInput: round(cachedPerCall + outputPerCall, 4),
      newInput: round(newPerCall, 4),
      total: round(cachedPerCall + outputPerCall + newPerCall, 4),
      medianCachedTokens: median(later.map(step => step.cacheRead)),
    };
  }
  const facts = rows.map(row => row.judge?.facts?.[arm]).filter(Array.isArray);
  if (facts.length) {
    const count = state => sum(facts.map(list => list.filter(item => item === state).length));
    const correct = facts.filter(list => list.every(item => item === 'right')).length;
    out.facts = { answers: facts.length, right: count('right'), missing: count('missing'), wrong: count('wrong'), unsupported: count('unsupported'), allRight: correct, costPerAllRight: correct ? round(sum(rows.filter(row => Array.isArray(row.judge?.facts?.[arm])).map(row => row.metrics.cost)) / correct, 3) : null };
  }
  for (const other of others) {
    const tally = { won: 0, tied: 0, lost: 0, unresolved: 0 };
    for (const row of rows) {
      const verdict = row.judge?.vs?.[`${arm}>${other}`];
      if (verdict) tally[verdict] += 1;
    }
    if (sum(Object.values(tally))) out[`vs_${other}`] = tally;
  }
  const failures = Object.entries(ledger.failures);
  const tokens = ledger.tokens;
  const table = entries => Object.fromEntries(Object.entries(entries).toSorted(([, a], [, b]) => (b.chars ?? b.calls ?? b.requests ?? 0) - (a.chars ?? a.calls ?? a.requests ?? 0)).map(([key, amounts]) => [key, Object.fromEntries(Object.entries(amounts).map(([name, amount]) => [name, Math.round(amount)]))]));
  out.ledger = {
    tools: table(ledger.tools),
    fields: table(ledger.fields),
    filters: table(ledger.filters),
    fileReads: { ...table(ledger.fileReads), rangeLines: ledger.rangeLines },
    cursor: ledger.cursor,
    failures: { refusals: sum(failures.filter(([cause]) => cause.startsWith('refused')).map(([, row]) => row.calls)), errors: sum(failures.filter(([cause]) => !cause.startsWith('refused')).map(([, row]) => row.calls)), byCause: table(ledger.failures) },
    newInputTokens: { questionAndUncachedPrompt: Math.round(tokens.prompt), modelsOwnTurns: Math.round(tokens.modelTurn), replies: Math.round(tokens.replies), total: Math.round(tokens.prompt + tokens.modelTurn + tokens.replies), providerReported: out.newTotal },
    replyCharsPerToken: tokens.replies ? round(out.charsTotal / tokens.replies) : null,
    firstCallCachedTokens: { median: out.firstCallCacheRead, min: Math.min(...runs.map(run => run.firstCallCacheRead)), max: Math.max(...runs.map(run => run.firstCallCacheRead)), cold: out.coldPrompt },
  };
  return out;
}

/** Ratios of pair [a, b] over the questions where both arms have a run, in the given passes. */
function ratios(per, names, passes, [a, b], random) {
  const out = {};
  for (const [metric] of RATIO_METRICS) {
    const pairs = [];
    for (const name of names) {
      const both = passes.map(pass => per[name]?.passes[pass]?.arms).filter(arms => arms?.[a] && arms?.[b]);
      if (both.length) pairs.push([sum(both.map(arms => arms[a][metric])), sum(both.map(arms => arms[b][metric])), name]);
    }
    const stat = pairedRatio(pairs, random);
    out[metric] = Object.fromEntries(Object.entries(stat).map(([key, value]) => [key, Number.isInteger(value) ? value : round(value, 3)]));
    if (metric === 'cost' && pairs.length) {
      const gaps = pairs.map(([x, y, name]) => ({ name, gap: x - y })).toSorted((p, q) => Math.abs(q.gap) - Math.abs(p.gap));
      const total = sum(gaps.map(item => item.gap));
      out[metric].gapTotal = round(total, 3);
      out[metric].topGaps = gaps.slice(0, 3).map(item => ({ name: item.name, gap: round(item.gap, 3) }));
    }
  }
  return out;
}

export function analyze(manifest) {
  const per = {};
  const sessions = [];
  const passes = Array.from({ length: manifest.passes }, (_, i) => i + 1);
  for (const question of questions(manifest)) {
    const title = fs.existsSync(question.file) ? fs.readFileSync(question.file, 'utf8').split('\n')[0].split('—').at(-1).replace(/^#\s*/, '').trim() : question.id;
    const entry = { title, set: question.set, id: question.id, passes: {} };
    for (const pass of passes) {
      const arms = {};
      for (const arm of question.arms) {
        const dir = runPath(manifest, pass, question.set, question.id, arm);
        if (!fs.existsSync(path.join(dir, 'result.json'))) continue;
        arms[arm] = runMetrics(dir, manifest.arms[arm].kind);
        sessions.push({ ...arms[arm].usage, cost: arms[arm].cost });
      }
      if (Object.keys(arms).length) entry.passes[pass] = { arms, judge: judged(path.join(passPath(manifest, pass), 'judge'), question.name, question.arms) ?? null };
    }
    per[question.name] = entry;
  }
  if (!sessions.length) throw new HarnessError(`no stored run under ${manifest.campaign}`);
  const prices = fitPrices(sessions.filter(session => session.cost > 0));
  const armNames = Object.keys(manifest.arms);
  const rowsOf = (names, onPasses, arm) => names.flatMap(name => onPasses.map(pass => per[name]?.passes[pass]).filter(entry => entry?.arms[arm]).map(entry => ({ metrics: entry.arms[arm], judge: entry.judge })));
  const block = (names, onPasses, seedTag) => {
    const arms = {};
    for (const arm of armNames) {
      const rows = rowsOf(names, onPasses, arm);
      if (rows.length) arms[arm] = summarize(arm, rows, armNames.filter(other => other !== arm), prices);
    }
    const compare = {};
    for (const pair of manifest.compare) {
      if (arms[pair[0]] && arms[pair[1]]) compare[pair.join('/')] = ratios(per, names, onPasses, pair, seeded(`${manifest.seed}-${seedTag}-${pair.join('/')}`));
    }
    return { arms, ratios: compare };
  };
  const groups = {};
  const allNames = Object.keys(per);
  for (const [name, group] of [...Object.entries(manifest.groups).map(([key, value]) => [key, value.ids.map(id => `${value.set}-${id}`)]), ['(all questions)', allNames]]) {
    const pooled = block(Array.isArray(group) ? group : [], passes, name);
    if (!Object.keys(pooled.arms).length) continue;
    groups[name] = { questions: group.length, ...pooled };
    if (passes.length > 1) {
      groups[name].perPass = Object.fromEntries(passes.map(pass => [pass, block(group, [pass], `${name}-pass-${pass}`)]));
      // A question whose cost ratio is below 1 in one pass and above 1 in another.
      groups[name].signFlips = Object.fromEntries(manifest.compare.map(([a, b]) => [`${a}/${b}`, group.filter(question => {
        const signs = new Set(passes.map(pass => per[question]?.passes[pass]?.arms).filter(arms => arms?.[a] && arms?.[b]).map(arms => Math.sign(arms[a].cost - arms[b].cost)));
        return signs.has(1) && signs.has(-1);
      })]));
    }
  }
  // Each run's steps and raw ledger served the sums above; the stored per-question rows keep the scalars.
  for (const entry of Object.values(per)) for (const pass of Object.values(entry.passes)) for (const metrics of Object.values(pass.arms)) for (const key of ['steps', 'ledger', 'usage']) delete metrics[key];
  return { schemaVersion: SCHEMA_VERSION, round: manifest.round, passes: manifest.passes, resamples: RESAMPLES, prices: prices && { perMillion: prices.perMillion, sessions: prices.sessions, worstRelativeError: prices.worstRelativeError }, groups, per };
}

const k = value => `${(value / 1000).toFixed(1)}k`;
const label = arm => (arm === 'gh' ? '`gh`' : arm);
const ratioText = stat => (stat.geomean === undefined ? 'no paired question' : `${stat.geomean.toFixed(2)} (${stat.low.toFixed(2)}–${stat.high.toFixed(2)})`);

export function tables(numbers) {
  const lines = [`# ${numbers.round}: analysis`, ''];
  if (numbers.prices) lines.push(`Prices fitted from ${numbers.prices.sessions} sessions (US$ per million tokens): ${Object.entries(numbers.prices.perMillion).map(([key, value]) => `${key} ${value}`).join(', ')}; worst session error ${(numbers.prices.worstRelativeError * 100).toFixed(2)}%.`, '');
  for (const [name, group] of Object.entries(numbers.groups)) {
    lines.push(`## ${name}`, '', '| Arm | Answers | Quality | Tool calls (failed) | Model calls | Reply characters | New input | Cached re-reads | Output | Cost |', '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
    for (const [arm, s] of Object.entries(group.arms)) lines.push(`| ${label(arm)} | ${s.n} | ${s.quality ?? '—'} | ${s.calls} (${s.failed}) | ${s.modelCalls} | ${s.charsTotal.toLocaleString('en-US')} | ${s.newTotal.toLocaleString('en-US')} | ${s.cachedTotal.toLocaleString('en-US')} | ${s.outputTotal.toLocaleString('en-US')} | $${s.cost.toFixed(2)} |`);
    for (const [pair, stats] of Object.entries(group.ratios)) {
      const [a, b] = pair.split('/');
      lines.push('', `### ${label(a)} / ${label(b)}, per question`, '', `| Measure | Geometric mean (95% interval) | Median | Range | ${label(a)} lower | ${label(b)} lower | Questions |`, '|---|---:|---:|---:|---:|---:|---:|');
      for (const [metric, text] of RATIO_METRICS) {
        const stat = stats[metric];
        lines.push(`| ${text} | ${ratioText(stat)} | ${stat.median?.toFixed(2) ?? '—'} | ${stat.min === undefined ? '—' : `${stat.min.toFixed(2)}–${stat.max.toFixed(2)}`} | ${stat.aLower} | ${stat.bLower} | ${stat.n}${stat.skipped ? ` (${stat.skipped} left out)` : ''} |`);
      }
      if (group.perPass) lines.push('', `Cost ratio per pass: ${Object.entries(group.perPass).map(([pass, data]) => `pass ${pass} ${data.ratios[pair] ? ratioText(data.ratios[pair].cost) : '—'}`).join('; ')}. Questions whose cost ratio changes side between passes: ${group.signFlips[pair].join(', ') || 'none'}.`);
    }
    for (const [arm, s] of Object.entries(group.arms)) {
      const ledger = s.ledger;
      const share = (amount, total) => (total ? `${((100 * amount) / total).toFixed(1)}%` : '—');
      lines.push('', `### Ledger: ${label(arm)}`, '', `New input tokens: ${ledger.newInputTokens.total.toLocaleString('en-US')} attributed (question and uncached prompt ${ledger.newInputTokens.questionAndUncachedPrompt.toLocaleString('en-US')}, the model's own turns ${ledger.newInputTokens.modelsOwnTurns.toLocaleString('en-US')}, replies ${ledger.newInputTokens.replies.toLocaleString('en-US')}) against ${ledger.newInputTokens.providerReported.toLocaleString('en-US')} reported by the provider. Reply text: ${ledger.replyCharsPerToken ?? '—'} characters per token. First-call cached tokens: median ${ledger.firstCallCachedTokens.median.toLocaleString('en-US')} (${ledger.firstCallCachedTokens.min.toLocaleString('en-US')} to ${ledger.firstCallCachedTokens.max.toLocaleString('en-US')}; ${ledger.firstCallCachedTokens.cold} cold).`);
      if (s.extraModelCall) lines.push('', `One more model call: $${s.extraModelCall.withoutNewInput.toFixed(4)} (cached re-read $${s.extraModelCall.cachedReread.toFixed(4)} + output $${s.extraModelCall.output.toFixed(4)}), plus $${s.extraModelCall.newInput.toFixed(4)} for the new input it reads on average; from ${s.extraModelCall.laterCalls} calls after the first.`);
      lines.push('', '| Tool | Calls (failed) | Characters | Share | Tokens |', '|---|---:|---:|---:|---:|');
      for (const [tool, row] of Object.entries(ledger.tools)) lines.push(`| \`${tool}\` | ${row.calls} (${row.failed}) | ${row.chars.toLocaleString('en-US')} | ${share(row.chars, s.charsTotal)} | ${row.tokens.toLocaleString('en-US')} |`);
      lines.push('', '| Output field | Characters | Share | Tokens |', '|---|---:|---:|---:|');
      for (const [field, row] of Object.entries(ledger.fields)) lines.push(`| \`${field}\` | ${row.chars.toLocaleString('en-US')} | ${share(row.chars, s.charsTotal)} | ${row.tokens.toLocaleString('en-US')} |`);
      if (Object.keys(ledger.filters).length) {
        lines.push('', '| Piped through | Calls | Characters |', '|---|---:|---:|');
        for (const [filter, row] of Object.entries(ledger.filters)) lines.push(`| \`${filter.replaceAll('|', '\\|')}\` | ${row.calls} | ${row.chars.toLocaleString('en-US')} |`);
      }
      const { rangeLines: lineCount, ...modes } = ledger.fileReads;
      lines.push('', `File requests by mode: ${Object.entries(modes).map(([mode, row]) => `${mode} ${row.requests}`).join(', ') || 'none'}${lineCount ? `; ${lineCount.toLocaleString('en-US')} lines asked for by range` : ''}. Cursor calls: ${ledger.cursor.calls} (${ledger.cursor.chars.toLocaleString('en-US')} characters).`);
      lines.push(`Failed calls: ${ledger.failures.refusals} refusals, ${ledger.failures.errors} errors${Object.keys(ledger.failures.byCause).length ? ` (${Object.entries(ledger.failures.byCause).map(([cause, row]) => `${cause} ${row.calls}`).join('; ')})` : ''}.`);
      if (s.facts) lines.push(`Facts: ${s.facts.right} right, ${s.facts.missing} missing, ${s.facts.wrong} wrong, ${s.facts.unsupported} unsupported over ${s.facts.answers} answers; ${s.facts.allRight} answers with every fact right${s.facts.costPerAllRight === null ? '' : ` ($${s.facts.costPerAllRight} each)`}.`);
    }
    lines.push('');
  }
  lines.push('## Per question', '', '| Pass | Question | Title | Arm | Correct | Calls (failed) | Characters | Time | New input | Cached re-read | Output | Cost |', '|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const [name, entry] of Object.entries(numbers.per)) {
    for (const [pass, data] of Object.entries(entry.passes)) {
      for (const [arm, r] of Object.entries(data.arms)) {
        const score = data.judge?.score?.[arm];
        lines.push(`| ${pass} | ${name} | ${entry.title.replaceAll('|', '\\|')} | ${label(arm)} | ${score === undefined ? '—' : score} | ${r.calls} (${r.failed}) | ${r.chars.toLocaleString('en-US')} | ${r.time.toFixed(0)} s | ${k(r.new)} | ${k(r.cached)} | ${k(r.output)} | $${r.cost.toFixed(3)} |`);
      }
    }
  }
  return `${lines.join('\n')}\n`;
}

export function summary(numbers) {
  const lines = [];
  for (const [name, group] of Object.entries(numbers.groups)) {
    lines.push(`== ${name} (${group.questions} questions, ${numbers.passes} pass${numbers.passes === 1 ? '' : 'es'})`);
    for (const [arm, s] of Object.entries(group.arms)) {
      const ledger = s.ledger;
      lines.push(`  ${arm}: ${s.n} answers, quality ${s.quality ?? '—'}, ${s.calls} calls (${s.failed} failed: ${ledger.failures.refusals} refusals, ${ledger.failures.errors} errors), ${s.modelCalls} model calls, ${s.charsTotal.toLocaleString('en-US')} reply characters, new ${s.newTotal.toLocaleString('en-US')}, cached ${s.cachedTotal.toLocaleString('en-US')}, output ${s.outputTotal.toLocaleString('en-US')}, $${s.cost.toFixed(2)}; first-call cached tokens ${ledger.firstCallCachedTokens.median}${s.extraModelCall ? `; one more model call $${s.extraModelCall.withoutNewInput.toFixed(4)}` : ''}`);
    }
    for (const [pair, stats] of Object.entries(group.ratios)) {
      const [a, b] = pair.split('/');
      for (const [metric, text] of RATIO_METRICS) lines.push(`  ${text} ratio ${pair} ${ratioText(stats[metric])}, ${a} lower on ${stats[metric].aLower}/${stats[metric].n}, ${b} lower on ${stats[metric].bLower}/${stats[metric].n}`);
    }
  }
  return lines.join('\n');
}

export async function main(argv) {
  const { options, positional } = parseArgs(argv, { flags: ['--json'], values: ['--out'] });
  if (positional.length !== 1) throw new HarnessError('pass one round manifest (see --help)');
  const manifest = loadManifest(positional[0]);
  const numbers = analyze(manifest);
  const out = options.out ? path.resolve(options.out) : path.join(manifest.campaign, 'analysis');
  fs.mkdirSync(out, { recursive: true });
  fs.writeFileSync(path.join(out, 'numbers.json'), `${JSON.stringify(numbers, null, 1)}\n`);
  fs.writeFileSync(path.join(out, 'tables.md'), tables(numbers));
  console.log(options.json ? JSON.stringify(numbers, null, 1) : `${summary(numbers)}\nwrote ${path.join(out, 'numbers.json')} and tables.md`);
  return 0;
}

if (isMain(import.meta.url)) await cli(main, HELP);
