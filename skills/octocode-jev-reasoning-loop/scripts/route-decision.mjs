#!/usr/bin/env node
import { routeDecision, DEFAULT_POLICY } from './decision-contract.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/route-decision.mjs --input FILE|- [--policy FILE] [--pretty]\nDeterministically selects the narrowest research route before any Jev call.');
  process.exit(0);
}
try {
  const options = parseFlags(process.argv.slice(2), ['--input', '--policy'], ['--pretty']);
  if (!options['--input']) throw new Error('--input is required.');
  const policy = options['--policy'] ? readJson(options['--policy']) : DEFAULT_POLICY;
  print(routeDecision(readJson(options['--input']), policy), options['--pretty']);
} catch {
  stop('Cannot route decision. Supply valid JSON up to 4 MiB with --input; use --help.');
}
