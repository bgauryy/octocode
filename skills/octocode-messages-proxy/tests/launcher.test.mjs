import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { fakeProcesses } from './fake-host.mjs';

const relay = fileURLToPath(new URL('../scripts/relay.py', import.meta.url));

function launch(t, args, { auth = { loggedIn: true }, promptText, processes, command = 'start', models = [
  { slug: 'full-size', visibility: 'list' },
  { slug: 'hidden-nano', visibility: 'hide' },
  { slug: 'available-mini', visibility: 'list' },
] } = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-proxy-launch-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  let launcher = relay;
  if (promptText !== undefined) {
    const bundle = path.join(dir, 'copied-skill');
    mkdirSync(path.join(bundle, 'scripts'), { recursive: true });
    mkdirSync(path.join(bundle, 'references'));
    for (const script of readdirSync(path.dirname(relay)).filter(name => name.endsWith('.py'))) {
      copyFileSync(path.join(path.dirname(relay), script), path.join(bundle, 'scripts', script));
    }
    launcher = path.join(bundle, 'scripts', 'relay.py');
    writeFileSync(path.join(bundle, 'references', 'proxy-prompts.md'), promptText);
  }
  const executable = (name, source) => writeFileSync(path.join(dir, name),
    `#!/usr/bin/env python3\nimport json,sys\n${source}`, { mode: 0o700 });
  executable('claude', `
if sys.argv[1:] == ['auth', 'status']:
    print(${JSON.stringify(JSON.stringify(auth))})
else:
    print(json.dumps({'command':sys.argv[1:]}))
`);
  executable('codex', `
if sys.argv[1:] == ['login', 'status']:
    sys.exit(0)
elif sys.argv[1:] == ['debug', 'models']:
    print(${JSON.stringify(JSON.stringify({ models }))})
else:
    print(json.dumps({'command':sys.argv[1:]}))
`);
  executable('octocode-agents-communication', "print(json.dumps({'command':sys.argv[1:]}))\n");
  fakeProcesses(dir, typeof processes === 'function' ? processes(dir) : processes);
  const task = command === 'start' ? ['--task', 'Relay only my messages to the named worker and return its replies.'] : [];
  return spawnSync('python3', [launcher, command, '--workspace', dir, ...task, ...args], {
    cwd: dir, encoding: 'utf8',
    env: { ...process.env, PATH: dir + path.delimiter + process.env.PATH, PYTHONDONTWRITEBYTECODE: '1' },
  });
}

function option(argv, name) {
  assert.ok(argv.includes(name), `Missing ${name} in ${JSON.stringify(argv)}`);
  return argv[argv.indexOf(name) + 1];
}

test('auto skips Claude loggedIn:false even when auth status exits successfully', t => {
  const result = launch(t, [], { auth: { loggedIn: false } });
  assert.equal(result.status, 0, result.stderr);
  const { command } = JSON.parse(result.stdout);
  assert.equal(option(command, '--vendor'), 'codex');
  assert.equal(option(command, '--model'), 'available-mini');
});

test('an explicitly selected Claude proxy refuses an unauthenticated account', t => {
  const result = launch(t, ['--vendor', 'claude'], { auth: { loggedIn: false } });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /signed-in CLI/);
  assert.equal(result.stdout, '');
});

test('an unrecognized Claude auth response does not authorize a model launch', t => {
  const result = launch(t, [], { auth: 'unknown status' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(option(JSON.parse(result.stdout).command, '--vendor'), 'codex');
});

test('Claude managed proxy uses Haiku and only the messaging profile', t => {
  const result = launch(t, ['--vendor', 'claude', '--name', 'status-proxy']);
  assert.equal(result.status, 0, result.stderr);
  const { command } = JSON.parse(result.stdout);
  assert.deepEqual(command.slice(0, 2), ['/cli', 'run']);
  assert.equal(option(command, '--vendor'), 'claude');
  assert.equal(option(command, '--model'), 'haiku');
  assert.equal(option(command, '--tools'), 'messaging');
  assert.equal(option(command, '--name'), 'status-proxy');
});

test('Codex refuses automatic launch when no visible small model exists', t => {
  const result = launch(t, ['--vendor', 'codex'], { models: [
    { slug: 'full-size', visibility: 'list' }, { slug: 'hidden-mini', visibility: 'hide' },
  ] });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /small model/);
  assert.equal(result.stdout, '');
});

test('an explicit account model requires an explicit vendor', t => {
  const ambiguous = launch(t, ['--model', 'account-mini']);
  assert.notEqual(ambiguous.status, 0);
  assert.match(ambiguous.stderr, /select --vendor/);
  const selected = launch(t, ['--vendor', 'codex', '--model', 'account-mini']);
  assert.equal(selected.status, 0, selected.stderr);
  assert.equal(option(JSON.parse(selected.stdout).command, '--model'), 'account-mini');
});

test('dry-run shows the resolved route without starting its launcher', t => {
  const result = launch(t, ['--vendor', 'codex', '--dry-run']);
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.equal(plan.vendor, 'codex');
  assert.equal(plan.model, 'available-mini');
  assert.equal(plan.route, 'local');
  assert.equal(path.basename(plan.argv[0]), 'octocode-agents-communication');
  assert.equal(option(plan.argv, '--tools'), 'messaging');
});

test('native Codex proxy has read-only repository access and low reasoning', t => {
  const result = launch(t, ['--vendor', 'codex', '--native']);
  assert.equal(result.status, 0, result.stderr);
  const { command } = JSON.parse(result.stdout);
  assert.equal(option(command, '-m'), 'available-mini');
  assert.equal(option(command, '-s'), 'read-only');
  assert.ok(command.includes('model_reasoning_effort="low"'));
});

// An agent CLI that runs this launcher through a shell, as a Bash tool does.
const under = (executable, command = executable) => ({ selfParent: 9001, rows: [
  [9001, 9002, '/bin/zsh', '/bin/zsh -c relay'], [9002, 1, executable, command]] });

test('auto prefers the operator host found in the process tree over the default order', t => {
  const result = launch(t, [], { processes: under('/opt/homebrew/bin/codex') });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(option(JSON.parse(result.stdout).command, '--vendor'), 'codex');
});

test('an operator that cannot host a proxy ranks the worker vendor, then Claude', t => {
  const octocode = under('node', 'octocode-main-abcd1234');
  const toCodex = launch(t, ['--worker-vendor', 'codex', '--dry-run'], { processes: octocode });
  assert.equal(toCodex.status, 0, toCodex.stderr);
  assert.deepEqual(JSON.parse(toCodex.stdout).operator, { vendor: 'octocode', pid: 9002, via: 'process' });
  assert.equal(JSON.parse(toCodex.stdout).vendor, 'codex');
  const unknownWorker = launch(t, ['--dry-run'], { processes: octocode });
  assert.equal(JSON.parse(unknownWorker.stdout).vendor, 'claude');
});

test('--operator-vendor overrides the process tree', t => {
  const result = launch(t, ['--operator-vendor', 'codex', '--dry-run'], { processes: under('/usr/local/bin/claude') });
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.deepEqual([plan.operator.via, plan.vendor], ['flag', 'codex']);
});

test('a native proxy takes the worker vendor, and refuses without one', t => {
  const result = launch(t, ['--native', '--worker-vendor', 'claude']);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(option(JSON.parse(result.stdout).command, '--model'), 'haiku');
  const unknown = launch(t, ['--native', '--worker-vendor', 'cursor']);
  assert.notEqual(unknown.status, 0);
  assert.match(unknown.stderr, /same vendor/);
});

test('detect names the nearest agent ancestor, ignoring helpers and the research CLI', t => {
  const result = launch(t, [], { command: 'detect', processes: { selfParent: 9001, rows: [
    [9001, 9002, '/usr/local/bin/claude', '/usr/local/bin/claude --chrome-native-host'],
    [9002, 9003, '/repo/npm/darwin-arm64/octocode', '/repo/npm/darwin-arm64/octocode mcp'],
    [9003, 9004, 'node', 'octocode-main-abcd1234'],
    [9004, 1, '/opt/homebrew/bin/codex', '/opt/homebrew/bin/codex'],
  ] } });
  assert.equal(result.status, 0, result.stderr);
  const { operator, proxy } = JSON.parse(result.stdout);
  assert.deepEqual(operator, { vendor: 'octocode', pid: 9003, via: 'process' });
  assert.deepEqual(proxy.map(row => row.vendor), ['claude', 'codex']);
});

test('an agent run from the Cursor editor counts only when no agent CLI is an ancestor', t => {
  const editor = { selfParent: 9001, rows: [
    [9001, 9002, '/bin/zsh', '/bin/zsh'],
    [9002, 1, '/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Plugin).app/Contents/MacOS/Cursor Helper (Plugin)', 'Cursor Helper (Plugin)'],
  ] };
  const result = launch(t, [], { command: 'detect', processes: editor });
  assert.deepEqual(JSON.parse(result.stdout).operator, { vendor: 'cursor', pid: 9002, via: 'editor' });
  editor.rows[0] = [9001, 9002, 'node', 'node /home/u/.local/share/cursor-agent/versions/1/index.js'];
  const cli = launch(t, [], { command: 'detect', processes: editor });
  assert.deepEqual(JSON.parse(cli.stdout).operator, { vendor: 'cursor', pid: 9001, via: 'process' });
});

test('detect lists each running agent session once with its workspace membership', t => {
  const result = launch(t, [], { command: 'detect', processes: dir => ({ rows: [
    [9100, 1, 'node', 'node /usr/lib/node_modules/@openai/codex/bin/codex.js'],
    [9101, 9100, '/usr/lib/node_modules/@openai/codex/vendor/codex', 'codex'],
    [9200, 1, '/Users/u/.grok/downloads/grok-1.0.77', 'grok'],
    [9300, 1, 'node', 'octocode-main-ffff0000'],
    [9400, 1, '/bin/zsh', 'zsh'],
  ], cwd: { 9100: dir, 9200: '/elsewhere' } }) });
  assert.equal(result.status, 0, result.stderr);
  const running = JSON.parse(result.stdout).running.map(({ vendor, pid, inWorkspace }) => [vendor, pid, inWorkspace]);
  assert.deepEqual(running, [['codex', 9100, true], ['grok', 9200, false], ['octocode', 9300, null]]);
});

const bundledPrompts = `<!-- relay-prompt:common -->
\`\`\`text
Fresh proxy instructions from its installed skill.
\`\`\`
<!-- /relay-prompt:common -->

<!-- relay-prompt:local -->
\`\`\`text
Use the bound Octocode messaging contract.
\`\`\`
<!-- /relay-prompt:local -->`;

test('a copied skill supplies the fresh proxy prompt without repository context', t => {
  const result = launch(t, ['--vendor', 'codex'], { promptText: bundledPrompts });
  assert.equal(result.status, 0, result.stderr);
  const prompt = option(JSON.parse(result.stdout).command, '--prompt');
  assert.match(prompt, /Fresh proxy instructions from its installed skill\./);
  assert.match(prompt, /Use the bound Octocode messaging contract\./);
  assert.match(prompt, /Relay only my messages to the named worker and return its replies\./);
});

test('native proxies receive common instructions without Octocode-only operations', t => {
  const result = launch(t, ['--vendor', 'codex', '--native'], { promptText: bundledPrompts });
  assert.equal(result.status, 0, result.stderr);
  const prompt = JSON.parse(result.stdout).command.at(-1);
  assert.match(prompt, /Fresh proxy instructions from its installed skill\./);
  assert.doesNotMatch(prompt, /Use the bound Octocode messaging contract\./);
});

test('missing or duplicate prompt blocks refuse launch instead of using stale instructions', t => {
  for (const promptText of ['', `${bundledPrompts}\n${bundledPrompts}`]) {
    const result = launch(t, ['--vendor', 'codex'], { promptText });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /prompt block/);
    assert.equal(result.stdout, '');
  }
});
