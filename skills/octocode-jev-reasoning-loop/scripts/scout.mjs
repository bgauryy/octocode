#!/usr/bin/env node
// scout.mjs — batched Jev scout: rank which candidate files the host should READ.
//
// STATUS: reference implementation and skills-only fallback. The production
// path is the native `jevScout` tool (octocode-core contract + crates/runtime
// tools/jev_scout.rs — see .octocode/rfc/jev-scout-production/). This file is
// the parity source of truth: the native port must reproduce its verdicts
// row-for-row on the frozen suites before any interface exposure.
//
// The host (System-2) supplies a capability claim, anchor patterns, and candidate
// files; the scout locates ALL anchor-matched spans per candidate server-side
// (sandboxed, redacted, bounded), sends ONE Jev request with a per-candidate
// Score question over an ordered relationship taxonomy, and maps the typed
// distributions to read / skip / gray_read actions. Candidate file bytes never
// enter the host's context; every verdict returns its span anchors and coverage.
//
// A scout PRIORITIZES reads. It never authorizes an irreversible action and its
// output is provisional — never citable evidence. Assert nothing from a scout
// verdict without reopening the anchors (see references/scout.md).
//
// POLICY v2 — FROZEN 2026-09-18 after the pilot eval
// (.octocode/octocode-eval-benchmark/jevpeek-scout/): do not tune thresholds or
// taxonomy against a suite this policy is being evaluated on.
//   no anchor matches            -> skip   (lexical parity with a search prefilter)
//   argmax == top level          -> read   (reads are cheap and reversible)
//   P(top level) <= 0.25         -> skip   (the anchored spans were judged and rejected)
//   otherwise                    -> gray_read (fail-open: the host reads it)
//
// Input JSON: {
//   claim: "computes a SHA-256 hash of the request",   // capability being located
//   anchors: ["createHash", "sha256"],                  // locate patterns (regex, case-insens)
//   candidates: ["a.mjs", "src/b.rs"],                  // root-relative files (2..12)
//   root?: ".",             // sandbox root (default: cwd); reads confined within it
//   levels?: [...],         // ordered taxonomy; default none/mentions/imports/implements
//   window?: 6, spanBudget?: 3000, model?: "jev-latest"
// }
// Usage: node scripts/scout.mjs --input scout.json [--dry-run] [--pretty] [--output DIR]

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { realpathSync } from 'node:fs';
import { boundedRoot, redact } from './resolve-content-ref.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';

const launcher = fileURLToPath(new URL('./jev.mjs', import.meta.url));
const DEFAULT_LEVELS = [
  { level: 'none', meaning: 'no relation to the capability' },
  { level: 'mentions', meaning: 'keywords appear but nothing is used' },
  { level: 'imports', meaning: 'imports or calls the capability defined elsewhere' },
  { level: 'implements', meaning: 'defines the capability itself in this file' }
];
// Relevance taxonomy for pre-fetched items (PRs, commits, issues) in items mode.
export const HISTORY_LEVELS = [
  { level: 'unrelated', meaning: 'does not concern the question' },
  { level: 'adjacent', meaning: 'touches the same area without bearing on the question' },
  { level: 'related', meaning: 'bears on the question but does not settle or address it' },
  { level: 'addresses', meaning: 'directly addresses the question; open this one' }
];
const WINDOW = 6, SPAN_BUDGET = 3000, MAX_SPANS = 12;
const T_SKIP = 0.25;

function mergeRanges(ranges) {
  ranges.sort((a, b) => a[0] - b[0]);
  const out = [];
  for (const r of ranges) {
    const last = out[out.length - 1];
    if (last && r[0] <= last[1] + 1) last[1] = Math.max(last[1], r[1]);
    else out.push([...r]);
  }
  return out;
}

// All anchor-matched spans in one file, merged, redacted, bounded (coverage-aware locate).
export function locateSpans(file, anchors, { rootDir, allowedRoots, window = WINDOW, spanBudget = SPAN_BUDGET } = {}) {
  const abs = boundedRoot(rootDir, allowedRoots, file);
  const raw = readFileSync(abs, 'utf8');
  const lines = raw.split('\n');
  const hits = [];
  for (const a of anchors) {
    const re = new RegExp(a, 'i');
    lines.forEach((l, i) => { if (re.test(l)) hits.push([Math.max(1, i + 1 - window), Math.min(lines.length, i + 1 + window)]); });
  }
  const spans = [];
  let judged = 0;
  for (const [s, e] of mergeRanges(hits)) {
    if (spans.length >= MAX_SPANS || judged >= spanBudget) break;
    let content = redact(lines.slice(s - 1, e).join('\n'));
    if (content.length > spanBudget - judged) content = content.slice(0, spanBudget - judged);
    if (!content.trim()) continue;
    judged += content.length;
    spans.push({ source: `${file}:L${s}-L${e}`, content });
  }
  return { spans, coverage: raw.length ? Number((judged / raw.length).toFixed(3)) : 0, fileChars: raw.length };
}

const questionId = (candidateIndex, dim, single) =>
  single ? `candidate_${candidateIndex}` : `candidate_${candidateIndex}__${dim.key}`;

// Multi-dimension support: several independent "courts" judge every candidate
// in the SAME request — the expensive spans are shared state, so extra
// dimensions cost only their question tokens, not another locate or call.
// Jev evaluates each question independently (no cross-question reasoning), so
// combining the score vector is deterministic host code, never another model.
// Roles keep frozen policy v2 intact:
//   primary — exactly one; drives read/skip via applyPolicy (unchanged)
//   veto    — may only DEMOTE a read to gray_read when its argmax is its
//             bottom level (forces MORE reading; can never create a skip)
//   info    — reported, never affects the action
function normalizeDimensions(input) {
  if (!Array.isArray(input.dimensions)) {
    return [{ key: 'main', role: 'primary', claim: input.claim, levels: input.levels || DEFAULT_LEVELS }];
  }
  if (input.dimensions.length < 1 || input.dimensions.length > 4) throw new Error('scout dimensions must be 1..4.');
  const dims = input.dimensions.map(d => ({ role: 'info', claim: input.claim, levels: DEFAULT_LEVELS, ...d }));
  if (dims.filter(d => d.role === 'primary').length !== 1) throw new Error('scout dimensions require exactly one primary.');
  if (new Set(dims.map(d => d.key)).size !== dims.length) throw new Error('scout dimension keys must be unique.');
  for (const d of dims) if (!d.key || !Array.isArray(d.levels) || d.levels.length < 2) throw new Error(`dimension ${d.key ?? '?'} requires key and >=2 levels.`);
  return dims;
}

// One request per scout run: shared state, one Score question per candidate
// per dimension, structured (non-prose) instructions and criteria.
export function buildScoutRequest(input, located) {
  const dims = normalizeDimensions(input);
  const state = { task: `For each candidate, judge every listed question about: ${input.claim}.`, candidates: {} };
  const questions = {};
  for (const [candidateIndex, [file, loc]] of Object.entries(located).entries()) {
    state.candidates[file] = loc.spans.length ? loc.spans : 'no anchor matches in this file';
    for (const dim of dims) {
      questions[questionId(candidateIndex, dim, dims.length === 1)] = {
        type: 'score',
        instructions: {
          judge: `${dim.key}: ${dim.claim}`,
          candidate: file,
          use_only: `state.candidates["${file}"]`,
          distinguish: 'importing or calling a capability defined elsewhere is NOT implementing it'
        },
        criteria: dim.levels
      };
    }
  }
  return { model: input.model || 'jev-latest', state, questions };
}

// FROZEN policy v2 mapping (see header). Top level = last taxonomy entry.
export function applyPolicy(answer, loc, levels) {
  if (!loc.spans.length) return { action: 'skip', reason: 'no_evidence' };
  const top = levels.length - 1;
  const probs = answer.probabilities || {};
  const pTop = probs[String(top)] ?? 0;
  const argmax = Object.entries(probs).reduce((m, [i, p]) => p > m[1] ? [Number(i), p] : m, [0, -1])[0];
  if (argmax === top) return { action: 'read', reason: 'argmax_top' };
  if (pTop <= T_SKIP) return { action: 'skip', reason: 'judged_and_rejected' };
  return { action: 'gray_read', reason: 'fail_open' };
}

// items mode: the host already holds cheap candidate rows (PR/commit/issue
// titles+bodies from a search); the scout judges those rows so only the top
// items get their expensive diffs opened. No locate stage; content is bounded
// and redacted exactly like file spans.
function locateItems(items, spanBudget = SPAN_BUDGET) {
  const located = {};
  for (const item of items) {
    if (!item.id || typeof item.content !== 'string' || !item.content.trim()) throw new Error('items mode requires { id, content } per item.');
    const content = redact(item.content.slice(0, spanBudget));
    located[item.id] = {
      spans: [{ source: item.source || item.id, content }],
      coverage: Number((content.length / item.content.length).toFixed(3)),
      fileChars: item.content.length
    };
  }
  return located;
}

export function runScout(input, options = {}) {
  const itemsMode = Array.isArray(input.items);
  if (!input.claim) throw new Error('scout input requires claim.');
  if (itemsMode === Boolean(Array.isArray(input.candidates))) throw new Error('scout input requires exactly one of candidates[] (files) or items[] (pre-fetched rows).');
  if (!itemsMode && (!Array.isArray(input.anchors) || !input.anchors.length)) throw new Error('scout input requires claim and anchors[].');
  const pool = itemsMode ? input.items : input.candidates;
  if (pool.length < 2 || pool.length > 12) throw new Error('scout input requires 2..12 candidates.');
  const identities = itemsMode ? pool.map(item => item?.id) : pool;
  if (identities.every(identity => typeof identity === 'string') && new Set(identities).size !== identities.length) {
    throw new Error('scout candidate paths or item IDs must be unique.');
  }
  if (normalizeDimensions(input).length * pool.length > 24) throw new Error('scout allows at most 24 questions (candidates x dimensions).');
  let located;
  if (itemsMode) {
    const itemSpanBudget = input.itemSpanBudget ?? input.spanBudget;
    if (input.itemSpanBudget !== undefined && (!Number.isInteger(input.itemSpanBudget) || input.itemSpanBudget < 200 || input.itemSpanBudget > 8000)) {
      throw new Error('scout itemSpanBudget must be an integer from 200 to 8000.');
    }
    if (input.itemSpanBudget === undefined && input.spanBudget !== undefined && (!Number.isInteger(input.spanBudget) || input.spanBudget < 1 || input.spanBudget > 8000)) {
      throw new Error('scout legacy spanBudget must be an integer from 1 to 8000.');
    }
    located = locateItems(input.items, itemSpanBudget);
  } else {
    const rootDir = resolve(input.root || process.cwd());
    const sandbox = { rootDir, allowedRoots: [rootDir], window: input.window, spanBudget: input.spanBudget };
    located = {};
    for (const file of input.candidates) located[file] = locateSpans(file, input.anchors, sandbox);
  }
  const request = buildScoutRequest(input, located);
  if (options.dryRun) {
    return { status: 'dry-run', request, candidates: Object.fromEntries(Object.entries(located).map(([f, l]) => [f, { spans: l.spans.map(s => s.source), coverage: l.coverage }])) };
  }
  const dir = options.output ? resolve(options.output) : join(process.cwd(), '.octocode', 'octocode-jev-reasoning-loop', `scout-${process.pid}`);
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const reqPath = join(dir, 'scout-request.json');
  writeFileSync(reqPath, JSON.stringify(request), { mode: 0o600 });
  const child = spawnSync(process.execPath, [launcher, 'evaluate', '--input', reqPath, '--project-env'],
    { encoding: 'utf8', timeout: 305000, maxBuffer: 5 * 1024 * 1024 });
  if (child.status !== 0) {
    const error = new Error(child.stderr.trim() || 'Jev evaluation failed; no scout verdicts are available.');
    error.exitCode = child.status ?? 3;
    throw error;
  }
  const response = JSON.parse(child.stdout);
  writeFileSync(join(dir, 'scout-response.json'), JSON.stringify(response, null, 2), { mode: 0o600 });
  const dims = normalizeDimensions(input);
  const primary = dims.find(d => d.role === 'primary');
  const results = {};
  let bytesOffHost = 0;
  for (const [candidateIndex, file] of Object.keys(located).entries()) {
    const loc = located[file];
    const answerFor = dim => response.answers[questionId(candidateIndex, dim, dims.length === 1)] || {};
    const answer = answerFor(primary);
    const policy = applyPolicy(answer, loc, primary.levels);
    // deterministic multi-court combiner: a veto court at its bottom level
    // demotes read -> gray_read (host reads anyway); it never creates a skip.
    let { action, reason } = policy;
    const dimensions = {};
    for (const dim of dims) {
      const a = answerFor(dim);
      dimensions[dim.key] = {
        role: dim.role,
        level: loc.spans.length ? dim.levels[Math.round(a.score ?? 0)]?.level : null,
        score: a.score ?? null, probabilities: a.probabilities ?? null
      };
      if (dim.role === 'veto' && action === 'read' && loc.spans.length) {
        const probs = a.probabilities || {};
        const argmax = Object.entries(probs).reduce((m, [i, p]) => p > m[1] ? [Number(i), p] : m, [0, -1])[0];
        if (argmax === 0) { action = 'gray_read'; reason = `vetoed_by_${dim.key}`; }
      }
    }
    bytesOffHost += loc.fileChars;
    results[file] = {
      action, reason,
      level: dimensions[primary.key].level,
      score: dimensions[primary.key].score, probabilities: dimensions[primary.key].probabilities,
      ...(dims.length > 1 ? { dimensions } : {}),
      coverage: loc.coverage, anchors: loc.spans.map(s => s.source),
      provisional: true
    };
  }
  return {
    status: 'scouted', claim: input.claim, model: response.model, results,
    reads: Object.keys(located).filter(f => results[f].action === 'read'),
    metrics: {
      jev: response.usage,
      candidate_bytes_kept_off_host: bytesOffHost,
      approx_host_tokens_saved_if_skips_hold: Math.ceil(Object.keys(located).filter(f => results[f].action === 'skip').reduce((s, f) => s + located[f].fileChars, 0) / 4)
    },
    artifacts: { request: reqPath, response: join(dir, 'scout-response.json') }
  };
}

function main(argv) {
  if (argv.some(a => ['--help', '-h'].includes(a))) {
    console.log('Usage: node scripts/scout.mjs --input scout.json [--dry-run] [--pretty] [--output DIR]\nBatched Jev scout: one call ranks which candidate files to read. Verdicts are provisional; reopen anchors before asserting.');
    return;
  }
  const options = parseFlags(argv, ['--input', '--output'], ['--dry-run', '--pretty']);
  if (!options['--input']) throw new Error('--input is required.');
  const result = runScout(readJson(options['--input']), { dryRun: options['--dry-run'], output: options['--output'] });
  print(result, options['--pretty']);
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(resolve(process.argv[1]))).href) {
  try { main(process.argv.slice(2)); }
  catch (error) { stop(`Cannot run scout: ${error instanceof Error ? error.message : 'invalid input'}`, Number(error?.exitCode) || 2); }
}
