#!/usr/bin/env node
import { validateDecisionPacket, DEFAULT_POLICY } from './decision-contract.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/validate-decision-packet.mjs --route ROUTE --input FILE|- [--policy FILE] [--pretty]\nValidates route-specific IDs, testable-hypothesis precommitments, outcomes, none options, evidence bases, and scope before a Jev request. Exit 0 valid; 2 input error; 4 rejected packet.');
  process.exit(0);
}
try {
  const options = parseFlags(process.argv.slice(2), ['--route', '--input', '--policy'], ['--pretty']);
  if (!options['--route'] || !options['--input']) throw new Error('--route and --input are required.');
  const policy = options['--policy'] ? readJson(options['--policy']) : DEFAULT_POLICY;
  const result = validateDecisionPacket(options['--route'], readJson(options['--input']), policy);
  print(result, options['--pretty']);
  process.exitCode = result.valid ? 0 : 4;
} catch (error) {
  stop(`Cannot validate decision packet: ${error instanceof Error ? error.message : 'invalid input'}. Supply --route and valid JSON up to 4 MiB; use --help.`);
}
