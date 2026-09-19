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
import { resolveEvidenceRefs } from './resolve-content-ref.mjs';
import { parseFlags, print, readJson, stop } from './cli-json.mjs';
import { getConfigSync } from './octocode-config.mjs';

const launcher = fileURLToPath(new URL('./jev.mjs', import.meta.url));
const LIMIT = 4 * 1024 * 1024;

function help() {
  console.log(`Usage: node scripts/run-loop.mjs --input compact.json [options]
   or: node scripts/run-loop.mjs --claim "..." --scope "..." --evidence path:S-E [options]

One entry point for deterministic routing, packet construction, validation,
Jev evaluation, claim consistency, and provisional APPLY.

Options:
  --claim TEXT       Review one unresolved semantic claim (hallucination_gate)
  --scope TEXT       Explicit boundary shared by the claim and selected evidence
  --evidence PATH:S-E Repeat for each supporting or opposing exact source span
  --goal TEXT        Claim-mode decision goal; otherwise derived mechanically
  --model NAME       Claim-mode model override; pin for comparisons
  --max-chars N      Claim-mode per-span bound (default 1200; maximum 4000)
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

// This shortcut derives metadata only. The caller still chooses the claim,
// applicable scope and complete evidence spans, including counterevidence.
export function buildClaimInput(options) {
  for (const [key, min, max] of [['--claim', 5, 1200], ['--scope', 1, 240]]) {
    if (typeof options[key] !== 'string' || options[key].trim().length < min || options[key].length > max) {
      throw new Error(`${key} requires ${min}..${max} characters.`);
    }
  }
  const refs = options['--evidence'];
  if (!Array.isArray(refs) || refs.length < 1 || refs.length > 12 || new Set(refs).size !== refs.length) {
    throw new Error('--evidence requires 1..12 distinct path:S-E spans; repeat the flag for counterevidence.');
  }
  const maxChars = options['--max-chars'] === undefined ? undefined : Number(options['--max-chars']);
  if (maxChars !== undefined && (!Number.isInteger(maxChars) || maxChars < 1 || maxChars > 4000)) {
    throw new Error('--max-chars requires an integer from 1 to 4000.');
  }
  if (options['--goal'] !== undefined && (!options['--goal'].trim() || options['--goal'].length > 800)) throw new Error('--goal requires 1..800 characters.');
  if (options['--model'] !== undefined && (!options['--model'].trim() || options['--model'].length > 120)) throw new Error('--model requires 1..120 characters.');
  return {
    route: 'hallucination_gate',
    ...(options['--model'] === undefined ? {} : { model: options['--model'] }),
    willChangeAction: true,
    state: {
      goal: options['--goal'] || 'Decide whether to state or qualify the supplied claim.',
      claim: options['--claim'], claim_scope: options['--scope'],
      evidence: refs.map((value, index) => {
        const match = /^(.*):(\d+(?:-\d+)?)$/.exec(value);
        if (!match || !match[1].trim()) throw new Error('--evidence expected a workspace-relative path:S or path:S-E.');
        return { id: `E${index + 1}`, scope: options['--scope'], contentRef: {
          path: match[1], lines: match[2], ...(maxChars === undefined ? {} : { maxChars })
        } };
      })
    }
  };
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

// R3 mitigation: contentRef reads are sandboxed to the workspace root plus any
// octocode local.allowedPaths / local.workspaceRoot. Falls back to the git root
// if config cannot be read, so a reasoning tool never reads arbitrary files.
function buildSandbox(cwd) {
  const root = workspaceRoot(cwd);
  const roots = new Set([root]);
  try {
    const local = getConfigSync()?.local;
    if (typeof local?.workspaceRoot === 'string' && local.workspaceRoot) roots.add(resolve(local.workspaceRoot));
    for (const p of Array.isArray(local?.allowedPaths) ? local.allowedPaths : []) {
      if (typeof p === 'string' && p) roots.add(resolve(p));
    }
  } catch { /* config unreadable: workspace root only */ }
  return { rootDir: root, allowedRoots: [...roots] };
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

function metricBlock(input, request, response, apiCalls, refs) {
  const block = {
    compact_input_bytes: Buffer.byteLength(JSON.stringify(input)),
    request_bytes: Buffer.byteLength(JSON.stringify(request)),
    api_calls: apiCalls,
    input_tokens: response?.usage?.input_tokens ?? 0,
    output_tokens: response?.usage?.output_tokens ?? 0
  };
  if (refs && refs.stats.refsResolved > 0) {
    block.content_ref = {
      refs_resolved: refs.stats.refsResolved,
      reference_input_bytes: refs.authoredBytes,       // canonical compact input before expansion
      inline_equivalent_input_bytes: refs.inlineBytes,
      input_bytes_avoided: refs.inlineBytes - refs.authoredBytes,
      file_chars_read_server_side: refs.stats.fileCharsRead,
      content_chars_injected: refs.stats.contentCharsInjected
    };
  }
  return block;
}

export function runLoop(input, options = {}, policy = DEFAULT_POLICY) {
  const authoredBytes = Buffer.byteLength(JSON.stringify(input));
  const sandbox = options.rootDir
    ? { rootDir: options.rootDir, allowedRoots: options.allowedRoots || [options.rootDir] }
    : buildSandbox(process.cwd());
  const resolution = resolveEvidenceRefs(input, sandbox);
  input = resolution.input;
  const refs = { stats: resolution.stats, authoredBytes, inlineBytes: Buffer.byteLength(JSON.stringify(input)) };
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
        metrics: metricBlock(input, prepared.request, undefined, 0, refs)
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
          metrics: metricBlock(input, prepared.request, response, apiCalls, refs)
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
      metrics: metricBlock(input, prepared.request, response, apiCalls, refs)
    }
  };
}

function main(argv) {
  if (argv.some(arg => ['--help', '-h'].includes(arg))) { help(); return; }
  const claimFlags = ['--claim', '--scope', '--evidence', '--goal', '--model', '--max-chars'];
  const options = parseFlags(argv, ['--input', '--output', '--response', '--policy', '--timeout-ms', '--retries', ...claimFlags], ['--dry-run', '--project-env', '--pretty'], ['--evidence']);
  if (options['--input'] && claimFlags.some(flag => Object.hasOwn(options, flag))) throw new Error('--input cannot combine with claim-mode flags.');
  if (!options['--input'] && !options['--claim']) throw new Error('Provide --input or --claim with --scope and --evidence.');
  for (const flag of ['--timeout-ms', '--retries']) {
    if (options[flag] !== undefined && (!/^\d+$/.test(options[flag]) || Number(options[flag]) < 0)) throw new Error(`${flag} expected a non-negative integer; received ${JSON.stringify(options[flag])}.`);
  }
  if (options['--response'] && statSync(options['--response']).size > LIMIT) throw new Error(`${options['--response']} exceeds 4 MiB.`);
  const input = options['--input'] ? readJson(options['--input']) : buildClaimInput(options);
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
