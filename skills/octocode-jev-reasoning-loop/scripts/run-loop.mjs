#!/usr/bin/env node
import { existsSync, mkdirSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import {
  applyResponse,
  buildRunApplication,
  DEFAULT_POLICY,
  prepareCompactRun
} from './decision-contract.mjs';
import { checkResearch, hashResearchRequest } from './check-research.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';

const launcher = fileURLToPath(new URL('./jev.mjs', import.meta.url));
const LIMIT = 4 * 1024 * 1024;

function help() {
  console.log(`Usage: node scripts/run-loop.mjs --input compact.json [options]

One entry point for deterministic routing, packet construction, validation,
Jev evaluation, claim consistency, and provisional APPLY.

Options:
  --output DIR       Artifact directory (default: <workspace>/.octocode/.../run-*)
  --response FILE    Use a recorded response; makes no API call
  --policy FILE      Override host policy
  --timeout-ms N     Forward the bounded Jev timeout
  --retries N        Forward the Jev retry count
  --dry-run          Build and native-validate only; makes no API call
  --project-env      Trust project .env through the shared config loader
  --pretty           Pretty-print the summary

Compact input keeps route-specific state but replaces DecisionBrief and action-map
boilerplate with: route, willChangeAction, state, and optional model, directCheck,
evidenceFresh, reasoning, actions, and netAction. A routed deterministic/no_jev
result exits without contacting Jev.`);
}

function workspaceRoot(start) {
  let current = resolve(start);
  while (true) {
    if (existsSync(join(current, '.git'))) return current;
    const parent = dirname(current);
    if (parent === current) return resolve(start);
    current = parent;
  }
}

function defaultOutput() {
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  return join(workspaceRoot(process.cwd()), '.octocode', 'octocode-jev-reasoning-loop', `run-${stamp}-${process.pid}`);
}

function writeJson(path, value) {
  writeFileSync(path, JSON.stringify(value, null, 2) + '\n', { mode: 0o600 });
}

function responseSummary(response) {
  return Object.fromEntries(Object.entries(response.answers || {}).map(([id, answer]) => [id, {
    type: answer.type,
    ...(answer.type === 'choice' ? { selected: answer.choice, probability: answer.probabilities?.[answer.choice] } : {}),
    ...(answer.type === 'noul' ? { value: answer.noul } : {}),
    ...(answer.type === 'score' ? { value: answer.score } : {})
  }]));
}

function evaluate(requestPath, options) {
  const args = [launcher, 'evaluate', '--input', requestPath];
  if (options['--dry-run']) args.push('--dry-run');
  if (options['--timeout-ms']) args.push('--timeout-ms', options['--timeout-ms']);
  if (options['--retries']) args.push('--retries', options['--retries']);
  if (options['--project-env']) args.push('--project-env');
  const child = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 305000, maxBuffer: 5 * 1024 * 1024 });
  if (child.status !== 0) {
    const error = new Error(child.stderr.trim() || 'Jev evaluation failed; no judgment is available.');
    error.exitCode = child.status ?? 3;
    throw error;
  }
  return JSON.parse(child.stdout);
}

function metricBlock(input, request, response, apiCalls) {
  return {
    compact_input_bytes: Buffer.byteLength(JSON.stringify(input)),
    request_bytes: Buffer.byteLength(JSON.stringify(request)),
    api_calls: apiCalls,
    input_tokens: response?.usage?.input_tokens ?? 0,
    output_tokens: response?.usage?.output_tokens ?? 0
  };
}

export function runLoop(input, options = {}, policy = DEFAULT_POLICY) {
  const prepared = prepareCompactRun(input, policy);
  if (prepared.status !== 'ready') {
    return {
      exitCode: 0,
      summary: {
        status: prepared.status,
        route: prepared.routing.route,
        policyAction: prepared.routing.policyAction,
        nextAction: prepared.nextAction,
        metrics: { compact_input_bytes: Buffer.byteLength(JSON.stringify(input)), request_bytes: 0, api_calls: 0, input_tokens: 0, output_tokens: 0 }
      }
    };
  }

  if (prepared.route === 'disputed_inference' && ['jev-latest', 'jev-preview'].includes(prepared.request.model)) {
    throw new Error('$.model expected a pinned version for disputed_inference; received an alias.');
  }

  const output = resolve(options.output || defaultOutput());
  mkdirSync(output, { recursive: true, mode: 0o700 });
  const requestPath = join(output, 'request.json');
  writeJson(requestPath, prepared.request);

  if (options.dryRun) {
    const dry = evaluate(requestPath, { '--dry-run': true, ...(options.projectEnv ? { '--project-env': true } : {}) });
    writeJson(join(output, 'dry-run.json'), dry);
    return {
      exitCode: 0,
      summary: {
        status: 'ready', route: prepared.route, policyAction: prepared.routing.policyAction,
        artifacts: { output, request: requestPath, dryRun: join(output, 'dry-run.json') },
        metrics: metricBlock(input, prepared.request, undefined, 0)
      }
    };
  }

  const response = options.response
    ? readJson(options.response)
    : evaluate(requestPath, {
      ...(options.timeoutMs ? { '--timeout-ms': options.timeoutMs } : {}),
      ...(options.retries ? { '--retries': options.retries } : {}),
      ...(options.projectEnv ? { '--project-env': true } : {})
    });
  const responsePath = join(output, 'response.json');
  writeJson(responsePath, response);
  const apiCalls = options.response ? 0 : 1;

  if (prepared.route === 'disputed_inference') {
    const check = checkResearch(prepared.request, response);
    const envelope = {
      protocol: 'octocode-jev-research/v2',
      requestSha256: hashResearchRequest(prepared.request),
      response,
      check
    };
    const envelopePath = join(output, 'research-envelope.json');
    writeJson(envelopePath, envelope);
    if (!check.usable) {
      return {
        exitCode: 4,
        summary: {
          status: 'blocked', route: prepared.route, reason: check.reason, nextAction: check.next,
          ...(check.suggestion ? { narrowing: check.suggestion } : {}),
          model: response.model, decisions: responseSummary(response),
          artifacts: { output, request: requestPath, response: responsePath, envelope: envelopePath },
          metrics: metricBlock(input, prepared.request, response, apiCalls)
        }
      };
    }
  }

  const generated = input.actions && input.netAction
    ? { actions: input.actions, netAction: input.netAction }
    : buildRunApplication(prepared.route, prepared.request, response, policy);
  const application = applyResponse(prepared.request, response, generated.actions, generated.netAction, policy);
  const applyPath = join(output, 'apply.json');
  writeJson(applyPath, application);
  return {
    exitCode: application.blocked ? 4 : 0,
    summary: {
      status: application.blocked ? 'blocked' : 'applied',
      route: prepared.route,
      model: response.model,
      decisions: responseSummary(response),
      netAction: application.net_action,
      blockReasons: application.block_reasons,
      artifacts: { output, request: requestPath, response: responsePath, apply: applyPath },
      metrics: metricBlock(input, prepared.request, response, apiCalls)
    }
  };
}

function main(argv) {
  if (argv.some(arg => ['--help', '-h'].includes(arg))) { help(); return; }
  const options = parseFlags(argv, ['--input', '--output', '--response', '--policy', '--timeout-ms', '--retries'], ['--dry-run', '--project-env', '--pretty']);
  if (!options['--input']) throw new Error('--input is required.');
  for (const flag of ['--timeout-ms', '--retries']) {
    if (options[flag] !== undefined && (!/^\d+$/.test(options[flag]) || Number(options[flag]) < 0)) throw new Error(`${flag} expected a non-negative integer; received ${JSON.stringify(options[flag])}.`);
  }
  if (options['--response'] && statSync(options['--response']).size > LIMIT) throw new Error(`${options['--response']} exceeds 4 MiB.`);
  const input = readJson(options['--input']);
  const policy = options['--policy'] ? readJson(options['--policy']) : DEFAULT_POLICY;
  const result = runLoop(input, {
    output: options['--output'], response: options['--response'], dryRun: options['--dry-run'],
    projectEnv: options['--project-env'], timeoutMs: options['--timeout-ms'], retries: options['--retries']
  }, policy);
  print(result.summary, options['--pretty']);
  process.exitCode = result.exitCode;
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(resolve(process.argv[1]))).href) {
  try { main(process.argv.slice(2)); }
  catch (error) {
    const code = Number(error?.exitCode) || 2;
    stop(`Cannot run reasoning loop: ${error instanceof Error ? error.message : 'invalid input'}`, code);
  }
}
