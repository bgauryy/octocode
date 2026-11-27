#!/usr/bin/env node
// `dev bench <run|judge|analyze|replay|test> …`: one entry for the benchmark harness (README.md in this folder).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const HERE = import.meta.dirname;
const COMMANDS = {
  run: 'start the sessions of a round (run.mjs)',
  judge: 'grade stored answers blind: first, then swap (judge.mjs)',
  analyze: 'numbers, paired ratios and the ledger of a stored round (analyze.mjs)',
  replay: 'compare the evidence two pinned binaries return for a round\'s stored calls (replay.mjs)',
  test: 'the harness\'s own tests',
};

const [command, ...rest] = process.argv.slice(2);
if (!command || command === '--help' || command === '-h' || !COMMANDS[command]) {
  if (command && !['--help', '-h'].includes(command)) console.error(`Unknown bench command "${command}".\n`);
  console.log(`Usage: node skills-dev/octocode-dev/scripts/dev.mjs bench <command> <round manifest> [options]\n\nCommands (each takes --help):\n${Object.entries(COMMANDS).map(([name, text]) => `  ${name.padEnd(8)} ${text}`).join('\n')}\n\nRound manifests: packages/octocode-benchmark/harness/rounds/`);
  process.exit(command === '--help' || command === '-h' ? 0 : 1);
}
const args = command === 'test' ? ['--test', ...fs.readdirSync(HERE).filter(name => name.endsWith('.test.mjs')).map(name => path.join(HERE, name)), ...rest] : [path.join(HERE, `${command}.mjs`), ...rest];
const { status, error } = spawnSync(process.execPath, args, { stdio: 'inherit' });
if (error) {
  console.error(`bench ${command}: ${error.message}`);
  process.exit(1);
}
process.exit(status ?? 1);
