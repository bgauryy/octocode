#!/usr/bin/env node
// code-scout.mjs — rank candidate files before expensive full reads.
// The docs/prose veto demotes read to gray_read when quoted code could be
// mistaken for an implementation. Both read and gray_read remain required.
//
//   Explicit candidates:
//   node scripts/code-scout.mjs \
//     --question "implements the jevScout read-prioritization policy v2" \
//     --anchors "gray_read,argmax,T_SKIP,veto,apply_policy" \
//     --candidates "a/mod.rs,b/jev.rs,docs/scout.md" [--root .] [--no-docs-veto] [--pretty]
//
//   One command from question -> reads (auto-collect candidates via git grep):
//   node scripts/code-scout.mjs --question "implements the retry backoff" \
//     --search "retry|backoff|sleep" [--path "src/**"] [--limit 12]
//
// --search runs `git grep -lIE <pattern>` from --root to collect candidate files,
// then scouts them; --search doubles as the anchor set when --anchors is omitted.
// If neither --anchors nor --search is given, searchable tokens (len >= 4) from
// the question are used as anchors. Verdicts are provisional: inspect decisive
// original evidence unless already inspected, complete and current, and never
// report absence from a skip alone. One batched Jev
// call covers up to 12 files.

import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { parseFlags, print, stop } from './cli-json.mjs';
import { runScout } from './scout.mjs';

const options = parseFlags(
  process.argv.slice(2),
  ['--question', '--anchors', '--candidates', '--search', '--path', '--limit', '--root', '--model', '--output'],
  ['--pretty', '--no-docs-veto', '--dry-run'],
);
if (!options['--question']) stop('--question is required.', 2);
if (!options['--candidates'] && !options['--search']) stop('Provide --candidates or --search.', 2);
if (options['--candidates'] && options['--search']) stop('Use --candidates or --search, not both.', 2);

const root = options['--root'] || '.';
const limit = Math.min(Math.max(parseInt(options['--limit'] ?? '12', 10) || 12, 2), 12);

let candidates;
let searchTruncated = false;
if (options['--candidates']) {
  candidates = options['--candidates'].split(',').map(value => value.trim()).filter(Boolean);
} else {
  // Auto-collect candidate files whose content matches the search pattern.
  const args = ['grep', '-lIE', '-e', options['--search']];
  if (options['--path']) args.push('--', options['--path']);
  let matched = [];
  try {
    matched = execFileSync('git', args, { cwd: resolve(root), encoding: 'utf8', timeout: 30000 })
      .split('\n').map(line => line.trim()).filter(Boolean);
  } catch (error) {
    // git grep exits 1 with no matches; anything else is a real failure.
    if ((error.status ?? 0) !== 1) stop(`git grep failed: ${String(error.message).split('\n')[0]}`, 3);
  }
  if (matched.length > limit) { searchTruncated = true; matched = matched.slice(0, limit); }
  candidates = matched;
}

if (candidates.length < 2 || candidates.length > 12) {
  stop(`Need 2..12 candidates; got ${candidates.length}${options['--search'] ? ' from --search (refine the pattern or --path)' : ''}.`, 2);
}
if (new Set(candidates).size !== candidates.length) stop('Candidate paths must be unique.', 2);

const anchors = (options['--anchors']
  ? options['--anchors'].split(',')
  : options['--search']
    ? options['--search'].split('|')
    : [...new Set((options['--question'].toLowerCase().match(/[a-z_][a-z0-9_]{3,}/g) || []))])
  .map(value => value.trim())
  .filter(Boolean);
if (!anchors.length) stop('No anchors: pass --anchors, a --search pattern, or a question with searchable terms.', 2);

const dimensions = [{ key: 'implements', role: 'primary', taxonomy: 'implements' }];
if (!options['--no-docs-veto']) {
  dimensions.push({
    key: 'is_code',
    role: 'veto',
    levels: [
      { level: 'prose_or_docs', meaning: 'this span is documentation, comments, or prose describing behavior' },
      { level: 'code_definition', meaning: 'this span is executable source code that defines the behavior' },
    ],
  });
}

let out;
try {
  out = runScout({ claim: options['--question'], root, candidates, anchors, dimensions, model: options['--model'] }, { dryRun: options['--dry-run'], output: options['--output'] });
} catch (error) {
  stop(`Scout failed: ${error.message.split('\n')[0]}`, error.exitCode ?? 3);
}
if (options['--dry-run']) {
  // Validated packet + located spans only; no API call, no verdicts.
  print({ status: 'dry-run', question: options['--question'], candidates_scouted: candidates, candidates: out.candidates, provisional: true }, options['--pretty']);
  process.exit(0);
}

const ranked = Object.entries(out.results)
  .map(([file, result]) => ({ file, action: result.action, level: result.level, score: result.score, reason: result.reason, probabilities: result.probabilities, coverage: result.coverage, truncated: result.truncated, anchors: result.anchors }))
  .sort((a, b) => (b.score ?? -1) - (a.score ?? -1));
const byAction = action => ranked.filter(row => row.action === action).map(row => row.file);

print({
  question: options['--question'],
  candidates_from: options['--search'] ? `git grep -lIE "${options['--search']}"` : 'explicit',
  candidates_scouted: candidates,
  read: byAction('read'),
  gray_read: byAction('gray_read'),
  skip: byAction('skip'),
  required_reads: ranked.filter(row => row.action !== 'skip').map(row => row.file),
  model: out.model,
  artifacts: out.artifacts,
  ranked,
  summary: `read ${ranked.filter(row => row.action !== 'skip').length} of ${ranked.length}; ${byAction('skip').length} skipped with bytes off host${searchTruncated ? ` (search truncated to first ${limit})` : ''}`,
  jev_usage: out.metrics?.jev,
  provisional: true,
}, options['--pretty']);
