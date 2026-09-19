#!/usr/bin/env node
// ask-file.mjs — packet-free multi-question interrogation of file content.
// Ask several independent yes/no questions about one or more sources in ONE Jev
// call per source; the host model never reads the content. Built on runProfile
// (all aspects batched per source, sources concurrent). Answers are typed
// probabilities P(yes) with a direction band; they are provisional and never
// citable evidence — reopen the source before asserting.
//
//   Local files (1..8, root-relative, sandboxed + redacted by the runner):
//   node scripts/ask-file.mjs --files "src/retry.ts,docs/retry.md" \
//     --questions "does it implement retry with backoff? || is it documentation rather than code? || does it own auth?"
//
//   Already-fetched content (e.g. a cached GitHub fetch) via stdin:
//   gh api repos/o/r/contents/x.ts -H "Accept: application/vnd.github.raw" | \
//     node scripts/ask-file.mjs --stdin --source "o/r@main:x.ts" --questions "q1 || q2 || q3"
//
// Questions split on '||' or newlines; 1..24 per source. Direction bands match
// the host policy in assets/default-policy.json: >=0.8 strong-yes, >=0.6
// lean-yes, (0.4,0.6) ambiguous — treat ambiguous as "read the source",
// <=0.4 lean-no, <=0.2 strong-no. For ordered rubrics or choice decks use
// profile.mjs with a full input packet instead.

import { parseFlags, print, stop } from './cli-json.mjs';
import { runProfile } from './profile.mjs';

const options = parseFlags(
  process.argv.slice(2),
  ['--questions', '--files', '--source', '--root', '--max-chars'],
  ['--stdin', '--pretty', '--dry-run'],
);
if (!options['--questions']) stop('--questions is required ("q1 || q2 || q3").', 2);
if (!options['--files'] && !options['--stdin']) stop('Provide --files or --stdin.', 2);
if (options['--files'] && options['--stdin']) stop('Use --files or --stdin, not both.', 2);

const questions = options['--questions']
  .split(/\|\||\n/)
  .map(value => value.trim())
  .filter(Boolean);
if (questions.length < 1 || questions.length > 24) stop('Provide 1..24 questions.', 2);

const aspects = questions.map((question, index) => ({
  key: `q${index + 1}`,
  type: 'noul',
  instructions: question,
  criteria: { true: 'The condition in the question holds for this source.', false: 'It does not hold for this source.' },
}));

let inputs;
if (options['--files']) {
  const files = options['--files'].split(',').map(value => value.trim()).filter(Boolean);
  if (files.length < 1 || files.length > 8) stop('Provide 1..8 files (comma-separated).', 2);
  if (new Set(files).size !== files.length) stop('File paths must be unique.', 2);
  inputs = files.map((path, index) => ({ id: `f${index + 1}`, path }));
} else {
  const content = await new Promise((resolveContent, rejectContent) => {
    let text = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', chunk => { text += chunk; });
    process.stdin.on('end', () => resolveContent(text));
    process.stdin.on('error', rejectContent);
  });
  if (!content.trim()) stop('stdin supplied no content.', 2);
  inputs = [{ id: 'f1', content, source: options['--source'] || 'stdin' }];
}

// Direction bands from assets/default-policy.json noul thresholds.
const direction = value =>
  value >= 0.8 ? 'strong-yes'
  : value >= 0.6 ? 'lean-yes'
  : value > 0.4 ? 'ambiguous'
  : value > 0.2 ? 'lean-no'
  : 'strong-no';

const input = {
  goal: 'Answer independent typed questions about each supplied source without host-model reads.',
  root: options['--root'] || '.',
  inputs,
  aspects,
};
if (options['--max-chars']) input.maxChars = parseInt(options['--max-chars'], 10);

let out;
try {
  out = await runProfile(input, { dryRun: options['--dry-run'] });
} catch (error) {
  stop(`Profile failed: ${String(error.message).split('\n')[0]}`, error.exitCode ?? 3);
}
if (options['--dry-run']) {
  // Validated requests only (content read, bounded, redacted); no API call.
  print({ status: 'dry-run', questions, requests: out.requests.map(entry => ({ id: entry.id, source: entry.source })), provisional: true }, options['--pretty']);
  process.exit(0);
}

const fileLabel = new Map(inputs.map(entry => [entry.id, entry.path ?? entry.source ?? entry.id]));
const results = out.results.map(result => ({
  source: fileLabel.get(result.id) ?? result.id,
  anchor: result.source,
  answers: questions.map((question, index) => {
    const answer = result.answers?.[`q${index + 1}`] ?? {};
    return { question, p_yes: answer.noul, direction: typeof answer.noul === 'number' ? direction(answer.noul) : 'missing' };
  }),
  usage: result.usage,
}));

print({
  questions,
  results,
  ambiguous_means: 'read the source yourself; do not force a yes/no',
  provisional: true,
}, options['--pretty']);
