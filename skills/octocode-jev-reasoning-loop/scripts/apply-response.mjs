#!/usr/bin/env node
import { applyResponse, DEFAULT_POLICY } from './decision-contract.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/apply-response.mjs --request FILE --response FILE --actions FILE --net-action TEXT [--policy FILE] [--pretty]\nCreates the mandatory provisional APPLY record. actions is a JSON map from every question ID to a concrete caller-owned action. Exit 0 applied; 2 invalid input; 4 blocked by policy.');
  process.exit(0);
}
try {
  const options = parseFlags(process.argv.slice(2), ['--request', '--response', '--actions', '--net-action', '--policy'], ['--pretty']);
  for (const key of ['--request', '--response', '--actions', '--net-action']) if (!options[key]) throw new Error(`${key} is required.`);
  const policy = options['--policy'] ? readJson(options['--policy']) : DEFAULT_POLICY;
  const result = applyResponse(readJson(options['--request']), readJson(options['--response']), readJson(options['--actions']), options['--net-action'], policy);
  print(result, options['--pretty']);
  process.exitCode = result.blocked ? 4 : 0;
} catch (error) {
  stop(`Cannot apply Jev response: ${error instanceof Error ? error.message : 'invalid input'}`);
}
