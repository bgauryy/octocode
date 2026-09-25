#!/usr/bin/env node
import { propagateOctocodeEnv } from '@octocodeai/config';
import { listModels, prepareExperiment, readExperiment, runExperiment } from './lib.mjs';

function usage() {
  return `Usage: yarn jev:probe (--input FILE | --models) [options]

Options:
  --input, -i FILE       JSON experiment manifest, or - for stdin
  --models               List models available to the configured account
  --repeat N             Number of measured requests (default: 1)
  --concurrency N        Parallel requests (default: 1)
  --timeout-ms N         Per-request timeout (default: 60000)
  --compact              Print compact JSON
  --help, -h             Show this help

Environment:
  OCTOCODE_CLASSIFICATION_API       Required provider key
  JEV_LAB_MODEL                     Optional lab model override (default: jev-latest)
  OCTOCODE_CLASSIFICATION_API_HOST  Optional HTTPS API root
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
    if (arg === '--models') {
      options.models = true;
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
  if (Boolean(options.input) === Boolean(options.models)) {
    throw new Error('provide exactly one of --input or --models.');
  }
  if (options.models && (options.repeat !== undefined || options.concurrency !== undefined)) {
    throw new Error('--repeat and --concurrency require --input.');
  }
  return options;
}

try {
  propagateOctocodeEnv();
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    process.exit(0);
  }
  if (options.models) {
    const result = await listModels(options);
    process.stdout.write(`${JSON.stringify(result.response, null, options.compact ? 0 : 2)}\n`);
    if (!result.ok) process.exitCode = 1;
  } else {
    const spec = await readExperiment(options.input);
    const prepared = await prepareExperiment(spec, options.input === '-' ? 'stdin.json' : options.input);
    const result = await runExperiment(prepared, options);
    process.stdout.write(`${JSON.stringify(result, null, options.compact ? 0 : 2)}\n`);
    if (result.summary.failures > 0) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`jev-lab: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
