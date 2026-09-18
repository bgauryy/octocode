#!/usr/bin/env node
import { buildDecisionPacket, DEFAULT_POLICY } from './decision-contract.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/build-decision-packet.mjs --input FILE|- [--policy FILE] [--pretty]\nBuilds and validates one public Jev request from a structured DecisionBrief. Prefer stdin so the brief stays ephemeral. Output contains only model, state, and questions.');
  process.exit(0);
}
try {
  const options = parseFlags(process.argv.slice(2), ['--input', '--policy'], ['--pretty']);
  if (!options['--input']) throw new Error('--input is required.');
  const policy = options['--policy'] ? readJson(options['--policy']) : DEFAULT_POLICY;
  print(buildDecisionPacket(readJson(options['--input']), policy), options['--pretty']);
} catch (error) {
  stop(`Cannot build decision packet: ${error instanceof Error ? error.message : 'invalid input'}`);
}
