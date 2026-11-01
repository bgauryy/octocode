// Shared protocol-suite harness: one engine call surface over the legacy engine CLI, MCP or
// typed CLI transports, plan helpers that record which methods reached Chrome, and the
// check runner. The entry assigns `suite` (work dir, ports, fixture base, transport) first.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { helpers } from '../../dist/engine/cli-catalog.mjs';

export const suite = {
  work: '',
  port: '',
  base: '',
  fixturePort: '',
  scripts: '',
  transport: 'legacy',
  client: null,
  only: undefined,
  protocol: undefined,
  browser: undefined,
  planValidatedCommands: 0,
  planValidatedEvents: 0,
};
export const results = [],
  tested = new Set(),
  advertised = new Set();

const camel = flag =>
  flag.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());

/** dev-check flags as its typed fields, on the suite port. */
function devCheckInput(argv) {
  const input = { port: Number(suite.port) };
  for (let i = 1; i < argv.length; i++) {
    const field = camel(argv[i]);
    if (['visible', 'fullPage', 'keepTab'].includes(field)) input[field] = true;
    else if (['url', 'waitUntil'].includes(field)) input[field] = argv[++i];
    else if (['timeoutMs', 'idleMs'].includes(field))
      input[field] = Number(argv[++i]);
    else throw Error('Unmapped dev-check argument ' + argv[i]);
  }
  return input;
}

/** Maps engine CLI arguments onto the tool input the MCP and typed CLI transports take. */
function toolInput(argv) {
  const name = argv[0],
    input = {},
    connection = { port: Number(suite.port) };
  if (name === 'dev-check') return { name, input: devCheckInput(argv) };
  if (['open', 'cleanup', 'artifact'].includes(name)) {
    input.args = [
      ...argv.slice(1),
      ...(name === 'artifact' ? [] : ['--port', suite.port]),
    ];
    return { name, input };
  }
  for (let i = 1; i < argv.length; i++) {
    const key = argv[i];
    if (key === '--json') input.plan = JSON.parse(argv[++i]);
    else if (key === '--plan')
      input.plan = JSON.parse(readFileSync(argv[++i], 'utf8'));
    else if (
      ['--new-tab', '--script-timeout', '--target', '--timeout'].includes(key)
    ) {
      const value = argv[++i],
        field = camel(key);
      connection[field] = ['scriptTimeout', 'timeout'].includes(field)
        ? Number(value)
        : value;
    } else if (['--browser', '--close-tab', '--keep-tab'].includes(key))
      connection[camel(key)] = true;
    else if (!key.startsWith('--') && name === 'protocol') input.member = key;
    else throw Error('Unmapped fixture argument ' + key);
  }
  input.connection = connection;
  return { name, input };
}

async function callTool(name, input, timeout) {
  if (suite.client) {
    const response = await suite.client.callTool(
      { name, arguments: input },
      undefined,
      { timeout }
    );
    return (
      response.structuredContent ??
      JSON.parse(response.content.find(row => row.type === 'text').text)
    );
  }
  const file = join(suite.work, 'typed-input.json');
  writeFileSync(file, JSON.stringify(input));
  const r = spawnSync(
    process.execPath,
    [
      join(suite.scripts, '../../bin/octocode-chrome-devtools.mjs'),
      '/cli',
      name,
      '--input',
      file,
      '--json',
    ],
    { cwd: suite.work, encoding: 'utf8', timeout }
  );
  if (r.error) throw r.error;
  const message = JSON.parse((r.stdout || r.stderr).trim());
  return message.structuredContent ?? message;
}

/** Runs one engine command; returns its exit code and combined stdout/stderr text. */
export async function cli(argv, timeout = 45000) {
  if (suite.transport !== 'legacy') {
    const { name, input } = toolInput(argv);
    const message = await callTool(name, input, timeout);
    const stdout =
      message.data !== undefined
        ? JSON.stringify(message.data)
        : readFileSync(
            (
              message.logs ??
              JSON.parse(readFileSync(message.capture.file, 'utf8')).logs
            ).stdout,
            'utf8'
          );
    const stderr = message.logs?.stderr
      ? readFileSync(message.logs.stderr, 'utf8')
      : '';
    return { code: message.exitCode, out: stdout + '\n' + stderr };
  }
  const r = spawnSync(
    process.execPath,
    [
      join(suite.scripts, 'cli.mjs'),
      ...argv,
      ...(Object.hasOwn(helpers, argv[0]) ? [] : ['--port', suite.port]),
    ],
    { cwd: suite.work, encoding: 'utf8', timeout }
  );
  return {
    code: r.status,
    out: (r.stdout ?? '') + (r.stderr ?? ''),
    error: r.error,
  };
}
export function artifact(out, name) {
  const prefix = '[ARTIFACT] ' + name + ' ',
    line = out.split('\n').find(row => row.startsWith(prefix));
  if (!line) throw Error('Missing ' + name + ': ' + out);
  return JSON.parse(readFileSync(line.slice(prefix.length), 'utf8'));
}
export const cdp = (method, params = {}, extra = {}) => ({
  op: 'cdp',
  method,
  params,
  ...extra,
});
export const ref = (step, pointer) => ({ $step: step, pointer }),
  event = (id, pointer) => ({ $event: id, pointer });
export async function plan(
  steps,
  selection = ['--new-tab', 'about:blank', '--close-tab'],
  options = {}
) {
  const r = await cli([
    'run',
    ...selection,
    '--script-timeout',
    '40000',
    '--json',
    JSON.stringify({ waitMs: 6000, commandMs: 6000, ...options, steps }),
  ]);
  if (r.error) throw r.error;
  const data = artifact(r.out, 'browser-result.json');
  for (const [i, row] of data.steps.entries())
    if (row.status === 'complete' && steps[i].op === 'cdp')
      tested.add(steps[i].method);
  if (r.code !== 0 || !data.ok) throw Error(r.out);
  return {
    data,
    out: r.out,
    read: i => JSON.parse(readFileSync(data.steps[i - 1].artifact, 'utf8')),
  };
}
export async function check(name, needs, fn) {
  if (suite.only !== undefined && !name.includes(suite.only)) return;
  const absent = needs.filter(method => !advertised.has(method));
  if (absent.length) {
    results.push({ name, status: 'unavailable', absent });
    console.log('UNAVAILABLE ' + name + ': ' + absent.join(', '));
    return;
  }
  const start = Date.now();
  try {
    await fn();
    results.push({ name, status: 'passed', ms: Date.now() - start });
    console.log('ok ' + name + ' (' + (Date.now() - start) + 'ms)');
  } catch (error) {
    results.push({
      name,
      status: 'failed',
      ms: Date.now() - start,
      error: error.message,
    });
    console.log('FAIL ' + name + ': ' + error.message);
  }
}
export function navigate(extra = []) {
  return [{ op: 'goto', url: suite.base, after: { selector: '#q' } }, ...extra];
}
export function events(result, id) {
  const file = result.data.eventCoverage.find(row => row.id === id).artifact;
  return readFileSync(file, 'utf8')
    .trim()
    .split('\n')
    .filter(Boolean)
    .map(line => JSON.parse(line));
}
/** Collects checks in definition order; the entry runs them through `check`. */
export function checkList() {
  const checks = [];
  return {
    checks,
    define: (name, needs, fn) => checks.push({ name, needs, fn }),
  };
}
/** Runs fn with `--target <id>` for a new browser-owned target at url, then closes it. */
export async function withTarget(url, fn) {
  const target = await plan(
    [cdp('Target.createTarget', { url })],
    ['--browser']
  );
  try {
    await fn(['--target', target.read(1).targetId]);
  } finally {
    await plan(
      [cdp('Target.closeTarget', { targetId: target.read(1).targetId })],
      ['--browser']
    );
  }
}
