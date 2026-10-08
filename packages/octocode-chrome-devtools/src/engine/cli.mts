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
  chromeExtractionFields,
  chromePlanExamples,
  chromeInputExamples,
} from './chrome-contract.mjs';

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
const fail = message => {
  throw new Error(message);
};
const json = (text, label) => {
  try {
    return JSON.parse(text);
  } catch {
    return fail(`${label} must be valid JSON`);
  }
};

function parse(args, values, booleans) {
  const opts = {},
    positional = [];
  for (let i = 0; i < args.length; i++) {
    const key = args[i];
    if (!key.startsWith('--')) {
      positional.push(key);
      continue;
    }
    if (!values.includes(key) && !booleans.includes(key))
      fail(`Unknown option ${key}; use --help`);
    if (Object.hasOwn(opts, key)) fail(`Duplicate option ${key}`);
    if (booleans.includes(key)) opts[key] = true;
    else {
      if (!args[i + 1] || args[i + 1].startsWith('--'))
        fail(`${key} needs a value`);
      opts[key] = args[++i];
    }
  }
  return { opts, positional };
}
const commonValues = [
  '--port',
  '--target',
  '--target-url',
  '--target-type',
  '--new-tab',
  '--timeout',
  '--script-timeout',
];
const commonBooleans = [
  '--browser',
  '--keep-tab',
  '--close-tab',
  '--no-reload',
  '--stealth',
  '--no-stealth',
  '--verbose',
  '--dry-run',
];
function runnerArgs(opts) {
  for (const key of ['--port', '--timeout', '--script-timeout'])
    if (
      opts[key] !== undefined &&
      (!/^\d+$/.test(opts[key]) ||
        !Number.isSafeInteger(Number(opts[key])) ||
        Number(opts[key]) < 1 ||
        (key === '--port' && Number(opts[key]) > 65535))
    )
      fail(
        `${key} needs a positive integer${key === '--port' ? ' <= 65535' : ''}`
      );
  if (opts['--keep-tab'] && opts['--close-tab'])
    fail('Use one of --keep-tab and --close-tab');
  if (opts['--stealth'] && opts['--no-stealth'])
    fail('Use one of --stealth and --no-stealth');
  const selectors = [
    '--target',
    '--target-url',
    '--new-tab',
    '--browser',
  ].filter(key => opts[key]);
  if (
    selectors.length > 1 ||
    (opts['--browser'] && (opts['--target-type'] || opts['--stealth']))
  )
    fail(
      'Use one target selector; --browser cannot combine with page selection or stealth'
    );
  if (opts['--new-tab'] && opts['--target-type'])
    fail('--new-tab cannot combine with --target-type');
  const args = [
    '--strict-target',
    '--no-reload',
    ...(opts['--close-tab'] ? [] : ['--keep-tab']),
  ];
  for (const key of [...commonValues, ...commonBooleans])
    if (
      opts[key] &&
      !['--dry-run', '--close-tab', '--keep-tab', '--no-reload'].includes(key)
    )
      args.push(key, ...(commonValues.includes(key) ? [opts[key]] : []));
  return args;
}
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
    if (args.includes('--help') || args.includes('-h')) {
      console.log(
        'Usage: octocode-cdp schema [command] [recipe|operation]\nExamples: schema run extract; schema check page-snapshot. No Chrome connection is made.'
      );
      return 0;
    }
    if (args.length > 2) fail('Usage: schema [command] [recipe|operation]');
    const [name, detail] = args;
    const recipe = name === 'check' ? detail : undefined;
    const operation = ['run', 'step'].includes(name) ? detail : undefined;
    if (name && !Object.hasOwn(commands, name)) fail(`Unknown command ${name}`);
    if (recipe && (name !== 'check' || !Object.hasOwn(checks, recipe)))
      fail(`Unknown recipe ${recipe}`);
    const { planOperations, actions } = await planContract();
    if (detail && !recipe && !operation)
      fail('Detail requires check recipe or run/step operation');
    if (operation && !Object.hasOwn(planOperations, operation))
      fail(
        `Unknown operation ${operation}; supported operations: ${Object.keys(planOperations).join(', ')}`
      );
    console.log(
      JSON.stringify(
        {
          commands: name ? { [name]: commands[name] } : commands,
          inputExamples:
            name && chromeInputExamples[name]
              ? { [name]: chromeInputExamples[name] }
              : !name
                ? chromeInputExamples
                : undefined,
          ...(!name ||
          !['open', 'cleanup', ...Object.keys(helpers)].includes(name)
            ? { connection }
            : {}),
          ...(name === 'open'
            ? { options: [...launchValues, '--headless', '--dry-run'] }
            : {}),
          ...(name === 'cleanup' ? { options: ['--port', '--dry-run'] } : {}),
          ...(planInputs[name] ? { inputs: planInputs[name] } : {}),
          ...(helpers[name]
            ? {
                help: {
                  command: process.execPath,
                  args: [fileURLToPath(import.meta.url), name, '--help'],
                },
                component: helpers[name][0],
              }
            : {}),
          ...(name === 'check' || !name
            ? { checks: recipe ? { [recipe]: checks[recipe] } : checks }
            : {}),
          ...(['run', 'step', 'cdp', 'protocol'].includes(name) || !name
            ? {
                plan: {
                  operations: operation
                    ? { [operation]: planOperations[operation] }
                    : planOperations,
                  ...(!operation || operation === 'act' ? { actions } : {}),
                  ...(!operation || operation === 'extract'
                    ? {
                        extraction: {
                          fields: chromeExtractionFields,
                          defaults: ['text', 'href'],
                          name: 'aria-label or name attribute; not computed accessible name',
                        },
                      }
                    : {}),
                  frameScopes: {
                    selector: ['act', 'wait', 'extract', 'cdp'],
                    isolatedTarget: ['act', 'wait', 'extract', 'cdp', 'listen'],
                  },
                  common: {
                    timeoutMs: 'Positive whole-step deadline',
                    after: 'Visible selector, text and/or exact url',
                    frame: 'id, url substring or selector (supported ops only)',
                    session:
                      'Flattened session id or an earlier result/event reference; mutually exclusive with frame',
                  },
                  options: {
                    steps:
                      'Nonempty array, runs in order and stops on first failure',
                    waitMs: 'Positive step deadline in ms',
                    commandMs:
                      'Positive request deadline, capped by remaining step time',
                    settleMs: 'Nonnegative delay; prefer content conditions',
                    traceEvents:
                      'Boolean, record event trust without typed characters',
                    observe: {
                      network: 'Boolean, subscribe before steps',
                      afterMs: 'Nonnegative additional observation window',
                    },
                  },
                  targets:
                    'ref, selector or exact role + name; ambiguity fails',
                  conditions:
                    'after is an object with nonempty selector, text and/or exact url',
                  frameOperations: ['act', 'wait', 'extract', 'cdp', 'listen'],
                  sessionOperations: [
                    'goto',
                    'act',
                    'wait',
                    'extract',
                    'cdp',
                    'listen',
                    'readStream',
                  ],
                  examples: operation
                    ? [chromePlanExamples[operation]]
                    : Object.values(chromePlanExamples),
                  defaults: { waitMs: 8000, settleMs: 0, commandMs: 'waitMs' },
                  references: {
                    $step:
                      'Earlier CDP step number (1-based), pointer: JSON pointer',
                    $event: 'Earlier listener id, pointer: JSON pointer',
                  },
                  example: {
                    steps: [
                      {
                        op: 'cdp',
                        method: 'Runtime.evaluate',
                        params: {
                          expression: 'document.title',
                          returnByValue: true,
                        },
                      },
                    ],
                  },
                  validation:
                    'run --plan file|- --dry-run validates with the executor before connecting',
                },
              }
            : {}),
        },
        null,
        2
      )
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
  const directory = join(process.cwd(), '.octocode/tmp/chrome-devtools/plans');
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
