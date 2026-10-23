#!/usr/bin/env node
import { spawn } from 'node:child_process';
import {
  readFileSync,
  existsSync,
  mkdirSync,
  writeFileSync,
  rmSync,
} from 'node:fs';
import { randomUUID } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  commands,
  checks,
  helpers,
  connection,
  launchValues,
  planInputs,
} from './cli-catalog.mjs';
import {
  commonBooleans,
  commonValues,
  fail,
  json,
  parse,
  runnerArgs,
} from './cli-options.mjs';
import { schemaOutput } from './cli-schema.mjs';

const scripts = dirname(fileURLToPath(import.meta.url));
const usage = `octocode-cdp <command> [options]\n${Object.entries(commands)
  .map(([name, desc]) => `  ${name.padEnd(17)} ${desc}`)
  .join('\n')}\n
Examples (use absolute package path from the workspace cwd):
  node <package>/bin/octocode-chrome-devtools.mjs /raw open --headless --url https://example.com
  node <package>/bin/octocode-chrome-devtools.mjs /raw targets
  node <package>/bin/octocode-chrome-devtools.mjs /raw snapshot --target <id>
  node <package>/bin/octocode-chrome-devtools.mjs /raw step --json '{"op":"act","role":"button","name":"Search","action":"click","after":{"text":"Results"}}' --target <id>
  node <package>/bin/octocode-chrome-devtools.mjs /raw run --plan /absolute/plan.json --dry-run
  node <package>/bin/octocode-chrome-devtools.mjs /raw protocol Runtime.evaluate --target <id>
  node <package>/bin/octocode-chrome-devtools.mjs /raw cdp Runtime.evaluate --params '{"expression":"document.title","returnByValue":true}' --target <id>
  node <package>/bin/octocode-chrome-devtools.mjs /raw query --file /absolute/capture.json --pointer /rows

Use <command> --help, schema [command], or schema check [name].
No npm install required. Exit 0: success; 2: invalid CLI input; runtime failures preserve engine exit codes.
Calls retain tabs and attached state. Multiple matching targets fail; use targets then --target.
`;
function execute(file, args, env = {}, dryRun = false) {
  const path = join(scripts, file);
  if (!existsSync(path))
    fail(
      `Missing CLI component ${file}; reinstall the complete Chrome package`
    );
  if (dryRun) {
    console.log(
      JSON.stringify(
        {
          command: process.execPath,
          args: [path, ...args],
          env,
          cwd: process.cwd(),
          executed: false,
        },
        null,
        2
      )
    );
    return Promise.resolve(0);
  }
  return new Promise<number>(resolveExit => {
    const child = spawn(process.execPath, [path, ...args], {
      stdio: 'inherit',
      env: { ...process.env, ...env },
    });
    const forward = signal => child.kill(signal);
    const interrupt = () => forward('SIGINT'),
      terminate = () => forward('SIGTERM');
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', terminate);
    child.once('error', error => {
      console.error(`[CDP_CLI] ${error.message}`);
      resolveExit(1);
    });
    child.once('exit', (code, signal) => {
      process.off('SIGINT', interrupt);
      process.off('SIGTERM', terminate);
      resolveExit(code ?? (signal === 'SIGINT' ? 130 : 143));
    });
  });
}
async function planContract() {
  return import('./cdp-checks/browser-execute.mjs');
}
async function main(argv) {
  const [command, ...args] = argv;
  if (!command || ['--help', '-h', 'help'].includes(command)) {
    console.log(usage);
    return 0;
  }
  if (!Object.hasOwn(commands, command))
    fail(`Unknown command ${command}; use --help`);
  if (command === 'schema') {
    console.log(
      await schemaOutput(args, planContract, fileURLToPath(import.meta.url))
    );
    return 0;
  }
  if (Object.hasOwn(helpers, command))
    return execute(helpers[command][0], args);
  if (['open', 'cleanup'].includes(command)) {
    const values = launchValues;
    if (args.includes('--help') || args.includes('-h'))
      return execute('open-browser.mjs', ['--help']);
    const { opts, positional } = parse(args, values, [
      '--headless',
      '--dry-run',
    ]);
    if (positional.length) fail('Unexpected positional argument');
    runnerArgs({ '--port': opts['--port'] });
    if (
      command === 'cleanup' &&
      Object.keys(opts).some(key => !['--port', '--dry-run'].includes(key))
    )
      fail('cleanup accepts --port and --dry-run');
    return execute(
      'open-browser.mjs',
      [...(command === 'cleanup' ? ['--cleanup'] : []), ...args],
      {},
      command === 'open' && opts['--dry-run']
    );
  }
  if (args.includes('--help') || args.includes('-h')) {
    console.log(
      `${command}: ${commands[command]}\n${Object.entries(connection)
        .map(([key, value]) => `  ${key} ${value}`)
        .join(
          '\n'
        )}\nInputs: run --plan file|- | --json JSON; step --json JSON; cdp Domain.method --params JSON; protocol [Domain[.member]]; check <name> --options JSON; script <path>.\nSnapshot/screenshot/check --options maps documented recipe option names to values; check schema check <name>. Recipe-specific flags follow --.\nExamples: see --help. Full plans and frames: docs/browser-execution.md.`
    );
    return 0;
  }
  const split = args.indexOf('--'),
    tail = split < 0 ? [] : args.slice(split + 1);
  const { opts, positional } = parse(
    split < 0 ? args : args.slice(0, split),
    [...commonValues, '--plan', '--json', '--params', '--options'],
    commonBooleans
  );
  const permitted = ['run', 'step', 'cdp', 'protocol'].includes(command)
    ? planInputs[command]
    : ['snapshot', 'screenshot', 'check'].includes(command)
      ? ['--options']
      : [];
  for (const key of ['--plan', '--json', '--params', '--options'])
    if (opts[key] !== undefined && !permitted.includes(key))
      fail(`${command} does not accept ${key}`);
  if (tail.length && !['check', 'script'].includes(command))
    fail(`${command} does not accept trailing recipe flags`);
  const runner = runnerArgs(opts),
    dry = !!opts['--dry-run'];
  const maxPositionals = ['cdp', 'protocol', 'check', 'script'].includes(
    command
  )
    ? 1
    : 0;
  if (positional.length > maxPositionals)
    fail('Unexpected positional argument');
  if (command === 'targets')
    return execute('cdp-sandbox.mjs', ['--list-targets', ...runner], {}, dry);
  if (command === 'script') {
    if (!positional[0]) fail('script needs a module path');
    const file = resolve(positional[0]);
    if (!existsSync(file) || !/\.(mjs|js)$/.test(file))
      fail('script needs an existing .mjs or .js module');
    return execute('cdp-sandbox.mjs', [file, ...runner, ...tail], {}, dry);
  }
  if (['snapshot', 'screenshot', 'check'].includes(command)) {
    const name =
      command === 'snapshot'
        ? 'page-snapshot'
        : command === 'screenshot'
          ? 'page-screenshot'
          : positional[0];
    if (
      tail.some(value =>
        [
          ...commonValues,
          ...commonBooleans,
          '--list-targets',
          '--strict-target',
          '--plan',
        ].includes(value)
      )
    )
      fail(
        'Connection options must precede --; trailing flags belong to the recipe'
      );
    if (!Object.hasOwn(checks, name))
      fail(`Unknown check ${name ?? '(missing)'}; use schema check`);
    const spec = checks[name],
      options = opts['--options'] ? json(opts['--options'], '--options') : {};
    if (!options || typeof options !== 'object' || Array.isArray(options))
      fail('--options needs an object');
    for (const [key, value] of Object.entries(options))
      if (
        !spec.options.includes(key) ||
        !['string', 'number', 'boolean'].includes(typeof value)
      )
        fail(`Unsupported ${name} option ${key}; use schema check ${name}`);
    const env = Object.fromEntries(
      Object.entries(options).map(([key, value]) => [
        key,
        typeof value === 'boolean' ? (value ? '1' : '0') : String(value),
      ])
    );
    return execute(
      'cdp-sandbox.mjs',
      [join(scripts, spec.file), ...runner, ...tail],
      env,
      dry
    );
  }
  let plan;
  if (command === 'run') {
    if (!!opts['--plan'] === !!opts['--json'])
      fail('run needs exactly one of --plan file|- and --json JSON');
    plan = json(
      opts['--json'] ??
        readFileSync(
          opts['--plan'] === '-' ? 0 : resolve(opts['--plan']),
          'utf8'
        ),
      'Plan'
    );
  } else if (command === 'step') {
    if (!opts['--json']) fail('step needs --json JSON');
    plan = { steps: [json(opts['--json'], 'Step')] };
  } else if (command === 'cdp') {
    if (!positional[0]) fail('cdp needs Domain.method');
    plan = {
      steps: [
        {
          op: 'cdp',
          method: positional[0],
          params: json(opts['--params'] ?? '{}', '--params'),
        },
      ],
    };
  } else if (command === 'protocol') {
    if (!positional[0])
      return execute(
        'cdp-sandbox.mjs',
        [join(scripts, 'cdp-checks/protocol-snapshot.mjs'), ...runner],
        {},
        dry
      );
    const parts = positional[0].split('.');
    if (parts.length > 2 || parts.some(p => !p))
      fail('protocol needs Domain or Domain.member');
    plan = {
      steps: [
        {
          op: 'protocol',
          domain: parts[0],
          ...(parts[1] ? { member: parts[1] } : {}),
        },
      ],
    };
  }
  const { validatePlan } = await planContract();
  validatePlan(plan);
  const serialized = JSON.stringify(plan);
  if (dry)
    return execute(
      'cdp-sandbox.mjs',
      [join(scripts, 'cdp-checks/browser-execute.mjs'), ...runner],
      { BROWSER_PLAN: serialized },
      true
    );
  const directory = join(
    process.cwd(),
    '.octocode/octocode-chrome-devtools/tmp/plans'
  );
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const file = join(directory, `plan-${randomUUID()}.json`);
  writeFileSync(file, serialized, { mode: 0o600, flag: 'wx' });
  try {
    return await execute('cdp-sandbox.mjs', [
      join(scripts, 'cdp-checks/browser-execute.mjs'),
      ...runner,
      '--plan',
      file,
    ]);
  } finally {
    rmSync(file, { force: true });
  }
}
try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  console.error(
    `[CDP_CLI] ${error.message}\nUse --help or schema for the supported inputs.`
  );
  process.exitCode = 2;
}
