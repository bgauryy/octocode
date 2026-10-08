#!/usr/bin/env node
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateFlags } from './cli-flags.mjs';
import { guideTopics as topics } from './cli-catalog.mjs';
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log(
    'Usage: skill [topic] [--offset N] [--length N]\nTopics: ' +
      topics.join(', ') +
      '\nReads package-owned guidance, with lossless artifact continuations.'
  );
} else {
  const topic =
    args[0] && !args[0].startsWith('--') ? args.shift() : 'operating';
  if (!topics.some(item => item === topic)) {
    console.error('Unknown guide topic ' + topic);
    process.exitCode = 2;
  } else {
    validateFlags(args, ['--offset', '--length']);
    const root = join(dirname(fileURLToPath(import.meta.url)), '../..');
    const file =
      topic === 'operating'
        ? join(root, 'OPERATING.md')
        : join(root, 'docs', topic + '.md');
    process.argv = [
      process.execPath,
      fileURLToPath(new URL('./artifact-query.mjs', import.meta.url)),
      '--file',
      file,
      '--format',
      'text',
      ...args,
    ];
    await import('./artifact-query.mjs');
  }
}
