#!/usr/bin/env node
// Compact source questions: Noul via --questions, or explicit typed aspects
// via --aspects JSON. One request per source; all questions are independent.
// Reopen source anchors before treating any judgment as evidence.

import { parseFlags, print, readJson, stop } from './cli-json.mjs';
import { runProfile } from './profile.mjs';
import { DEFAULT_POLICY } from './decision-contract.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/ask-file.mjs (--files a.ts,b.ts | --stdin) (--questions "q1 || q2" | --aspects aspects.json) [--context scope] [--model model] [--lines S-E] [--root dir] [--max-chars n] [--output dir] [--source label] [--dry-run] [--pretty]\nUse typed aspects with explicit unsupported/insufficient alternatives when yes/no would assume the feature exists.');
  process.exit(0);
}

const options = parseFlags(
  process.argv.slice(2),
  ['--questions', '--aspects', '--context', '--model', '--lines', '--output', '--files', '--source', '--root', '--max-chars'],
  ['--stdin', '--pretty', '--dry-run'],
);
if (Boolean(options['--questions']) === Boolean(options['--aspects'])) stop('Provide exactly one of --questions or --aspects.', 2);
if (options['--lines'] && !options['--files']) stop('--lines requires --files.', 2);
if (!options['--files'] && !options['--stdin']) stop('Provide --files or --stdin.', 2);
if (options['--files'] && options['--stdin']) stop('Use --files or --stdin, not both.', 2);

const questions = (options['--questions'] || '')
  .split(/\|\||\n/)
  .map(value => value.trim())
  .filter(Boolean);
if (options['--questions'] && (questions.length < 1 || questions.length > 24)) stop('Provide 1..24 questions.', 2);

const aspects = options['--aspects'] ? readJson(options['--aspects']) : questions.map((question, index) => ({
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
  inputs = files.map((path, index) => ({ id: `f${index + 1}`, path, ...(options['--lines'] ? { lines: options['--lines'] } : {}) }));
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
  value >= DEFAULT_POLICY.noul.strongYesMinimum ? 'strong-yes'
  : value >= DEFAULT_POLICY.noul.leanYesMinimum ? 'lean-yes'
  : value > DEFAULT_POLICY.noul.leanNoMaximum ? 'ambiguous'
  : value > DEFAULT_POLICY.noul.strongNoMaximum ? 'lean-no'
  : 'strong-no';

const input = {
  goal: 'Answer independent typed questions about each supplied source without host-model reads.',
  root: options['--root'] || '.',
  ...(options['--context'] ? { context: options['--context'] } : {}),
  ...(options['--model'] ? { model: options['--model'] } : {}),
  inputs,
  aspects,
};
if (options['--max-chars']) input.maxChars = Number(options['--max-chars']);

let out;
try {
  out = await runProfile(input, { dryRun: options['--dry-run'], output: options['--output'] });
} catch (error) {
  stop(`Profile failed: ${String(error.message).split('\n')[0]}`, error.exitCode ?? 3);
}
if (options['--dry-run']) {
  // Validated requests only (content read, bounded, redacted); no API call.
  print({ status: 'dry-run', questions, requests: out.requests.map(entry => ({ id: entry.id, source: entry.source, model: entry.request.model, context: entry.request.state.context, questions: entry.request.questions })), provisional: true }, options['--pretty']);
  process.exit(0);
}

const fileLabel = new Map(inputs.map(entry => [entry.id, entry.path ?? entry.source ?? entry.id]));
const results = out.results.map(result => ({
  source: fileLabel.get(result.id) ?? result.id,
  anchor: result.source,
  answers: aspects.map(aspect => {
    const answer = result.answers[aspect.key];
    return { key: aspect.key, question: aspect.instructions, ...answer,
      ...(answer.type === 'noul' ? { p_yes: answer.noul, direction: direction(answer.noul) } : {}) };
  }),
  model: result.model,
  artifacts: result.artifacts,
  usage: result.usage,
}));

print({
  questions: aspects.map(aspect => aspect.instructions),
  results,
  ambiguous_means: 'read the source yourself; do not force a yes/no',
  provisional: true,
}, options['--pretty']);
