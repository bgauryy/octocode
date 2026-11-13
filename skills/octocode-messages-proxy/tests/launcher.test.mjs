import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const relay = fileURLToPath(new URL('../scripts/relay.py', import.meta.url));

function launch(t, args, { auth = { loggedIn: true }, skillText, models = [
  { slug: 'full-size', visibility: 'list' },
  { slug: 'hidden-nano', visibility: 'hide' },
  { slug: 'available-mini', visibility: 'list' },
] } = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-proxy-launch-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  let launcher = relay;
  if (skillText !== undefined) {
    const bundle = path.join(dir, 'copied-skill');
    mkdirSync(path.join(bundle, 'scripts'), { recursive: true });
    launcher = path.join(bundle, 'scripts', 'relay.py');
    copyFileSync(relay, launcher);
    writeFileSync(path.join(bundle, 'SKILL.md'), skillText);
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
  return spawnSync('python3', [launcher, 'start', '--workspace', dir, '--task',
    'Relay only my messages to the named worker and return its replies.', ...args], {
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
  const result = launch(t, ['--vendor', 'codex'], { skillText: bundledPrompts });
  assert.equal(result.status, 0, result.stderr);
  const prompt = option(JSON.parse(result.stdout).command, '--prompt');
  assert.match(prompt, /Fresh proxy instructions from its installed skill\./);
  assert.match(prompt, /Use the bound Octocode messaging contract\./);
  assert.match(prompt, /Relay only my messages to the named worker and return its replies\./);
});

test('native proxies receive common instructions without Octocode-only operations', t => {
  const result = launch(t, ['--vendor', 'codex', '--native'], { skillText: bundledPrompts });
  assert.equal(result.status, 0, result.stderr);
  const prompt = JSON.parse(result.stdout).command.at(-1);
  assert.match(prompt, /Fresh proxy instructions from its installed skill\./);
  assert.doesNotMatch(prompt, /Use the bound Octocode messaging contract\./);
});

test('missing or duplicate prompt blocks refuse launch instead of using stale instructions', t => {
  for (const skillText of ['', `${bundledPrompts}\n${bundledPrompts}`]) {
    const result = launch(t, ['--vendor', 'codex'], { skillText });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /prompt block/);
    assert.equal(result.stdout, '');
  }
});
