#!/usr/bin/env node
// `flow list|show|run|save [name] [--params JSON] [--plan -|file]` plus connection flags: named, parametrized plans (see flows/flow-core.mts).
import { spawn } from 'node:child_process';
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromeCommands } from './chrome-contract.mjs';
import { errorMessage } from './error-message.mjs';
import {
  describe,
  expandFlow,
  getFlow,
  loadFlows,
  resolveParams,
  saveFlow,
  scrubText,
  secretValues,
  validateExpanded,
} from './flows/flow-core.mjs';
import {
  commonBooleans,
  commonValues,
  fail,
  json,
  optionValue,
  parse,
  runnerArgs,
} from './cli-options.mjs';

const actions = ['list', 'show', 'run', 'save'];
const namePattern = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;
/** Validated `flow` argv: action, optional name, params, the save plan source and the connection flags. */
export function parseFlowArgs(args: string[]) {
  const { opts, positional } = parse(
    args,
    [...commonValues, '--params', '--plan'],
    commonBooleans
  );
  const [action, name] = positional;
  if (!actions.includes(action) || positional.length > 2)
    fail(`flow needs an action: ${actions.join(', ')}`);
  if ((action === 'list') !== (name === undefined))
    fail(
      action === 'list' ? 'list does not take a name' : `${action} needs a name`
    );
  if (name !== undefined && !namePattern.test(name))
    fail('flow name: letters, digits, . _ -; at most 64 characters');
  if ((action === 'save') !== (opts['--plan'] !== undefined))
    fail(
      action === 'save'
        ? 'save needs --plan file|-'
        : '--plan applies only to save'
    );
  let paramsText = optionValue(opts, '--params');
  if (paramsText === '-' && opts['--plan'] === '-')
    fail('--params - and --plan - cannot both read stdin');
  // `-` reads stdin so secret values never sit in the process argument list.
  if (paramsText === '-') paramsText = readFileSync(0, 'utf8');
  const params =
    paramsText === undefined ? undefined : json(paramsText, '--params');
  if (
    params !== undefined &&
    (params === null ||
      typeof params !== 'object' ||
      Array.isArray(params) ||
      !Object.values(params).every(value =>
        ['string', 'number', 'boolean'].includes(typeof value)
      ))
  )
    fail('--params needs an object of string, number or boolean values');
  return {
    action,
    name,
    params,
    plan: optionValue(opts, '--plan'),
    connection: runnerArgs(opts),
  };
}

type Parsed = ReturnType<typeof parseFlowArgs>;
const print = (value: unknown) =>
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);

/** Connection flags exactly as given (without --params/--plan), for the `run` command. */
function rawConnection(args: string[]) {
  const { opts } = parse(
    args,
    [...commonValues, '--params', '--plan'],
    commonBooleans
  );
  return Object.entries(opts)
    .filter(([key]) => key !== '--params' && key !== '--plan')
    .flatMap(([key, value]) => (value === true ? [key] : [key, String(value)]));
}

/** Rewrites every JSON/text file a run saved so no secret value stays on disk. */
function scrubFolder(dir: string, secrets: string[]) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const file = join(dir, entry.name);
    if (entry.isDirectory()) scrubFolder(file, secrets);
    else if (/\.(json|jsonl|txt|log|har)$/.test(entry.name)) {
      const text = readFileSync(file, 'utf8'),
        clean = scrubText(text, secrets);
      if (clean !== text)
        writeFileSync(file, clean, { mode: statSync(file).mode & 0o777 });
    }
  }
}

function execute(planText: string, connection: string[], secrets: string[]) {
  const cli = join(dirname(fileURLToPath(import.meta.url)), 'cli.mjs');
  return new Promise<{ code: number; out: string; err: string }>(done => {
    const child = spawn(
      process.execPath,
      [cli, 'run', '--plan', '-', ...connection],
      { stdio: ['pipe', 'pipe', 'pipe'] }
    );
    let out = '',
      err = '';
    child.stdout.on('data', chunk => (out += chunk));
    child.stderr.on('data', chunk => (err += chunk));
    const forward = (signal: NodeJS.Signals) => child.kill(signal);
    process.on('SIGINT', forward);
    process.on('SIGTERM', forward);
    child.stdin.on('error', () => {});
    child.stdin.end(planText);
    child.once('error', error => {
      err += `${errorMessage(error)}\n`;
      done({ code: 1, out, err });
    });
    child.once('exit', (code, signal) => {
      process.off('SIGINT', forward);
      process.off('SIGTERM', forward);
      const folder = /^\[ARTIFACT\] STEPS (.+)$/m.exec(out)?.[1];
      if (folder && secrets.length) scrubFolder(dirname(folder), secrets);
      done({ code: code ?? (signal === 'SIGINT' ? 130 : 143), out, err });
    });
  });
}

/** Runs one parsed `flow` call; returns the exit code. */
export async function flowMain(args: string[], workspace = process.cwd()) {
  const parsed: Parsed = parseFlowArgs(args);
  const { action, name, params } = parsed;
  if (action === 'list') {
    const { flows, invalid } = loadFlows(workspace);
    print({
      flows: flows.map(({ def, source }) => describe(def, source)),
      ...(invalid.length ? { invalid } : {}),
    });
    return 0;
  }
  if (action === 'save') {
    const file = parsed.plan!;
    const text = readFileSync(file === '-' ? 0 : resolve(file), 'utf8');
    const saved = saveFlow(workspace, name, json(text, 'Plan'));
    print({
      saved: saved.def.name,
      file: saved.file,
      params: saved.def.params,
    });
    return 0;
  }
  const { def, source } = getFlow(workspace, name);
  if (action === 'show') {
    const { values, missing } = resolveParams(def, params, true);
    const { plan } = expandFlow(def, values, { redact: true, missing });
    let valid = true,
      error: string | undefined;
    try {
      validateExpanded(plan, secretValues(def, values));
    } catch (cause) {
      valid = false;
      error = errorMessage(cause);
    }
    print({
      flow: describe(def, source),
      ...(def.repeat ? { repeat: def.repeat } : {}),
      template: def.plan,
      missing,
      plan,
      valid,
      ...(error ? { error } : {}),
    });
    return 0;
  }
  const { values } = resolveParams(def, params);
  const secrets = secretValues(def, values);
  const { plan } = expandFlow(def, values);
  validateExpanded(plan, secrets);
  const result = await execute(
    JSON.stringify(plan),
    rawConnection(args),
    secrets
  );
  process.stdout.write(scrubText(result.out, secrets));
  process.stderr.write(scrubText(result.err, secrets));
  process.stdout.write(
    `[FLOW] ${JSON.stringify({ flow: def.name, expects: def.expects })}\n`
  );
  return result.code;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) {
    process.stdout.write(
      `flow: ${chromeCommands.flow}\nUsage: flow list | flow show|run <name> [--params JSON] | flow save <name> --plan file|-; connection flags as run\n`
    );
  } else {
    try {
      process.exitCode = await flowMain(args);
    } catch (error) {
      process.stderr.write(`[FLOW] ${errorMessage(error)}\n`);
      process.exitCode = 2;
    }
  }
}
