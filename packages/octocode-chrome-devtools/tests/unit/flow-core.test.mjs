// Flows without a browser: built-ins against the real plan validator, parameter refusal before any
// connection, placeholder placement, secret handling and the workspace store round trip.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  mkdtempSync,
  rmSync,
  statSync,
  writeFileSync,
  mkdirSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dist = join(import.meta.dirname, '../../dist/engine');
const core = await import(join(dist, 'flows/flow-core.mjs'));
const { builtinFlows } = await import(join(dist, 'flows/index.mjs'));
const { validatePlan } = await import(
  join(dist, 'cdp-checks/plan-validate.mjs')
);
const command = join(dist, 'flow-command.mjs');

const workspace = () => mkdtempSync(join(tmpdir(), 'octo-flow-'));
const flow = (cwd, ...args) =>
  spawnSync(process.execPath, [command, ...args], {
    cwd,
    encoding: 'utf8',
  });
const flowStdin = (cwd, stdin, ...args) =>
  spawnSync(process.execPath, [command, ...args], {
    cwd,
    encoding: 'utf8',
    input: stdin,
  });

test('every built-in validates and its sample expansion passes the real plan validator', () => {
  const names = builtinFlows.map(def => def.name);
  assert.equal(new Set(names).size, names.length);
  for (const name of [
    'console-network-audit',
    'extract-links',
    'fill-form',
    'full-page-screenshot',
    'login-form',
    'paginate-list',
  ])
    assert.ok(names.includes(name), name);
  for (const def of builtinFlows) {
    core.validateDefinition(def);
    const { values } = core.resolveParams(def, core.sampleParams(def));
    validatePlan(core.expandFlow(def, values).plan);
  }
});

test('paginate-list repeats steps maxPages-1 times and absent optional params drop their steps', () => {
  const def = builtinFlows.find(item => item.name === 'paginate-list');
  const run = pages => {
    const { values } = core.resolveParams(def, {
      itemSelector: 'li',
      nextName: 'Next',
      ...(pages === undefined ? {} : { maxPages: pages }),
    });
    return core.expandFlow(def, values).plan.steps;
  };
  const extracts = steps => steps.filter(step => step.op === 'extract').length;
  assert.equal(extracts(run(3)), 3);
  assert.equal(extracts(run(1)), 1);
  assert.equal(extracts(run()), def.params.maxPages.default);
  assert.ok(run(3).every(step => step.op !== 'goto'));
  assert.throws(
    () =>
      core.resolveParams(def, {
        itemSelector: 'li',
        nextName: 'n',
        maxPages: 'x',
      }),
    /number/
  );
  const zero = core.resolveParams(def, {
    itemSelector: 'li',
    nextName: 'n',
    maxPages: 0,
  });
  assert.throws(() => core.expandFlow(def, zero.values), /repeats/);
});

test('missing, unknown and mistyped params are refused before any connection', () => {
  const login = builtinFlows.find(item => item.name === 'login-form');
  assert.throws(() => core.resolveParams(login, {}), /Missing required/);
  assert.throws(
    () => core.resolveParams(login, { ...core.sampleParams(login), nope: 'x' }),
    /Unknown param nope/
  );
  assert.throws(
    () => core.resolveParams(login, { ...core.sampleParams(login), url: 5 }),
    /url must be a string/
  );
  const form = builtinFlows.find(item => item.name === 'fill-form');
  const half = core.resolveParams(form, {
    field1Label: 'a',
    field1Value: 'b',
    field2Label: 'c',
  });
  assert.throws(() => core.expandFlow(form, half.values), /together with/);
  // The command refuses without a browser: no port is reachable, yet the failure names the param.
  const cwd = workspace();
  try {
    const bad = flow(
      cwd,
      'run',
      'login-form',
      '--params',
      '{"bogus":"x"}',
      '--port',
      '1'
    );
    assert.equal(bad.status, 2);
    assert.match(bad.stderr, /Unknown param bogus/);
    const typed = flow(
      cwd,
      'run',
      'extract-links',
      '--params',
      '{"selector":7}',
      '--port',
      '1'
    );
    assert.equal(typed.status, 2);
    assert.match(typed.stderr, /selector must be a string/);
    const dry = flow(
      cwd,
      'run',
      'extract-links',
      '--params',
      '{"url":"https://example.com/"}',
      '--dry-run'
    );
    assert.equal(dry.status, 0, dry.stderr);
    assert.match(dry.stdout, /\[FLOW\]/);
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('placeholders are refused in keys and a string in a numeric position fails the real validator', () => {
  assert.throws(
    () => core.placeholders({ '{{k}}': 'x' }),
    /only inside string values/
  );
  const cwd = workspace();
  try {
    assert.throws(
      () =>
        core.saveFlow(cwd, 'bad-key', {
          steps: [{ op: 'goto', url: 'https://example.com/', '{{k}}': 1 }],
        }),
      /only inside string values/
    );
    assert.throws(
      () =>
        core.saveFlow(cwd, 'bad-number', {
          steps: [{ op: 'wait', selector: 'main', timeoutMs: '{{t}}' }],
        }),
      /timeoutMs|Invalid/i
    );
    assert.equal(core.loadFlows(cwd).flows.length, builtinFlows.length);
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('a secret value never appears in list, show, errors or the printed expansion', () => {
  const cwd = workspace();
  const secret = 'S3cr3t-pw-value';
  try {
    const params = JSON.stringify({
      userLabel: 'User',
      userValue: 'bob',
      passwordLabel: 'Password',
      passwordValue: secret,
      submitName: 'Go',
      successText: 'Welcome',
    });
    const show = flow(cwd, 'show', 'login-form', '--params', params);
    assert.equal(show.status, 0, show.stderr);
    assert.ok(!show.stdout.includes(secret) && !show.stderr.includes(secret));
    assert.match(show.stdout, /\[secret:passwordValue\]/);
    const shown = JSON.parse(show.stdout);
    assert.equal(shown.valid, true);
    assert.ok(shown.plan.steps.some(step => step.name === 'User'));
    const wrong = flow(
      cwd,
      'show',
      'login-form',
      '--params',
      JSON.stringify({ passwordValue: 5 })
    );
    assert.equal(wrong.status, 2);
    const list = flow(cwd, 'list');
    assert.ok(!list.stdout.includes(secret));
    for (const def of builtinFlows)
      for (const param of Object.values(def.params))
        if (param.secret) assert.equal(param.default, undefined);
    // Scrubbing covers raw, JSON-escaped and URL-encoded forms.
    const odd = 'a"b c';
    const text = `x ${odd} ${JSON.stringify(odd)} ${encodeURIComponent(odd)}`;
    assert.ok(!core.scrubText(text, [odd]).includes('b c'));
    // A validator error on the real plan is scrubbed too.
    assert.throws(
      () =>
        core.validateExpanded(
          { steps: [{ op: 'nope', value: secret }], x: secret },
          [secret]
        ),
      error => !error.message.includes(secret)
    );
    // Secrets cannot be defaulted in a definition.
    assert.throws(
      () =>
        core.validateDefinition({
          ...builtinFlows[0],
          params: {
            p: { type: 'string', secret: true, default: 'x', description: 'd' },
          },
          plan: { steps: [{ op: 'wait', selector: 'a' }] },
        }),
      /cannot have a default/
    );
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('save, list and show round trip; collisions, overwrites and traversal are refused', () => {
  const cwd = workspace();
  try {
    const plan = JSON.stringify({
      steps: [
        { op: 'goto', url: '{{target}}' },
        {
          op: 'act',
          action: 'fill',
          role: 'textbox',
          name: 'Key',
          value: '{{apiToken}}',
        },
      ],
    });
    const saved = flowStdin(cwd, plan, 'save', 'my-flow', '--plan', '-');
    assert.equal(saved.status, 0, saved.stderr);
    const file = join(
      cwd,
      '.octocode/octocode-chrome-devtools/flows/my-flow.json'
    );
    assert.equal(statSync(file).mode & 0o777, 0o600);
    const savedOut = JSON.parse(saved.stdout);
    assert.equal(savedOut.params.target.type, 'string');
    assert.equal(savedOut.params.apiToken.secret, true);

    const list = JSON.parse(flow(cwd, 'list').stdout);
    const mine = list.flows.find(item => item.name === 'my-flow');
    assert.equal(mine.source, 'workspace');
    assert.equal(
      list.flows.filter(item => item.source === 'built-in').length,
      builtinFlows.length
    );

    const show = JSON.parse(
      flow(
        cwd,
        'show',
        'my-flow',
        '--params',
        '{"target":"https://example.com/","apiToken":"tok-123456"}'
      ).stdout
    );
    assert.equal(show.plan.steps[0].url, 'https://example.com/');
    assert.ok(!JSON.stringify(show).includes('tok-123456'));
    assert.deepEqual(show.missing, []);

    const again = flowStdin(cwd, plan, 'save', 'my-flow', '--plan', '-');
    assert.equal(again.status, 2);
    assert.match(again.stderr, /already exists/);
    const builtin = flowStdin(cwd, plan, 'save', 'login-form', '--plan', '-');
    assert.equal(builtin.status, 2);
    assert.match(builtin.stderr, /built-in/);
    const invalid = flowStdin(
      cwd,
      '{"steps":[{"op":"nope"}]}',
      'save',
      'junk',
      '--plan',
      '-'
    );
    assert.equal(invalid.status, 2);

    for (const name of ['../evil', '..', 'a/b', '.hidden', '', 'x'.repeat(65)])
      assert.throws(
        () => core.saveFlow(cwd, name, JSON.parse(plan)),
        /flow name/
      );
    assert.throws(() => core.getFlow(cwd, '../x'), /flow name/);

    // A hand-placed file shadowing a built-in is reported, not served.
    mkdirSync(join(cwd, '.octocode/octocode-chrome-devtools/flows'), {
      recursive: true,
    });
    writeFileSync(
      join(cwd, '.octocode/octocode-chrome-devtools/flows/login-form.json'),
      '{}'
    );
    const after = JSON.parse(flow(cwd, 'list').stdout);
    assert.match(after.invalid[0].reason, /built-in/);
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});
