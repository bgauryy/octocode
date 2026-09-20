#!/usr/bin/env node
import { propagateOctocodeEnv } from '@octocodeai/config';
import { prepareExperiment, readExperiment, runExperiment } from './lib.mjs';

function usage() {
  return `Usage: yarn workspace @octocodeai/jev-lab probe --input FILE [options]

Options:
  --input, -i FILE       JSON experiment manifest, or - for stdin
  --repeat N             Number of measured requests (default: 1)
  --concurrency N        Parallel requests (default: 1)
  --timeout-ms N         Per-request timeout (default: 60000)
  --compact              Print compact JSON
  --help, -h             Show this help

Environment:
  OCTOCODE_JEV_KEY       Required provider key
  OCTOCODE_JEV_MODEL     Optional model override (default: jev-latest)
  OCTOCODE_JEV_BASE_URL  Optional HTTPS API root
`;
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') return { help: true };
    if (arg === '--compact') {
      options.compact = true;
      continue;
    }
    const field = {
      '--input': 'input',
      '-i': 'input',
      '--repeat': 'repeat',
      '--concurrency': 'concurrency',
      '--timeout-ms': 'timeoutMs',
    }[arg];
    if (!field) throw new Error(`unknown argument ${JSON.stringify(arg)}.`);
    const value = argv[index + 1];
    if (value === undefined) throw new Error(`${arg} requires a value.`);
    options[field] = value;
    index += 1;
  }
  if (!options.input) throw new Error('--input is required.');
  return options;
}

try {
  propagateOctocodeEnv();
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    process.exit(0);
  }
  const spec = await readExperiment(options.input);
  const prepared = await prepareExperiment(spec, options.input === '-' ? 'stdin.json' : options.input);
  const result = await runExperiment(prepared, options);
  process.stdout.write(`${JSON.stringify(result, null, options.compact ? 0 : 2)}\n`);
  if (result.summary.failures > 0) process.exitCode = 1;
} catch (error) {
  process.stderr.write(`jev-lab: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
