// CDP built-in flows without a browser: typed placeholders, param declarations, acknowledge, show output.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const dist = join(import.meta.dirname, '../../dist/engine');
const core = await import(join(dist, 'flows/flow-core.mjs'));
const { builtinFlows } = await import(join(dist, 'flows/index.mjs'));
const { validatePlan } = await import(
  join(dist, 'cdp-checks/plan-validate.mjs')
);
const command = join(dist, 'flow-command.mjs');
const flow = (...args) =>
  spawnSync(process.execPath, [command, ...args], { encoding: 'utf8' });
const def = name => builtinFlows.find(item => item.name === name);
const expand = (name, params) => {
  const flowDef = def(name);
  const { values } = core.resolveParams(flowDef, params);
  return core.expandFlow(flowDef, values).plan;
};

test('every built-in has an area, expands with sample params and passes the real validator', () => {
  const areas = ['emulation', 'inspection', 'input', 'permissions', 'audits'];
  for (const item of builtinFlows) {
    assert.ok(areas.includes(item.area), `${item.name} area`);
    const { values } = core.resolveParams(item, core.sampleParams(item));
    validatePlan(core.expandFlow(item, values).plan);
    if (item.sensitive) assert.equal(item.params.acknowledge.required, true);
  }
  assert.ok(builtinFlows.filter(item => item.sensitive).length >= 3);
});

test('a whole-value placeholder yields the typed value; an embedded one stays text', () => {
  const plan = expand('emulate-device', {
    width: 390,
    height: 844,
    touch: true,
  });
  const [metrics, touch] = plan.steps;
  assert.equal(metrics.params.width, 390);
  assert.equal(typeof metrics.params.mobile, 'boolean');
  assert.equal(touch.params.enabled, true);
  assert.equal(plan.steps.length, 2, 'absent userAgent drops its step');
  const withAgent = expand('emulate-device', {
    width: 1,
    height: 1,
    userAgent: 'UA/1',
  });
  assert.equal(withAgent.steps[2].params.userAgent, 'UA/1');
  const dialog = expand('handle-dialog', { trigger: 'alert(1)' });
  const evaluate = dialog.steps.find(
    step => step.method === 'Runtime.evaluate'
  );
  assert.equal(typeof evaluate.params.expression, 'string');
  assert.match(evaluate.params.expression, /alert\(1\)/);
  const wait = dialog.steps.find(step => step.op === 'waitEvent');
  assert.equal(wait.timeoutMs, def('handle-dialog').params.ms.default);
  assert.equal(typeof wait.timeoutMs, 'number');
});

test('absent optional params drop their step, key or list item', () => {
  const media = expand('emulate-media', { colorScheme: 'dark' });
  assert.deepEqual(media.steps[0].params.features, [
    { name: 'prefers-color-scheme', value: 'dark' },
  ]);
  const dialog = expand('handle-dialog', { accept: false });
  assert.ok(!dialog.steps.some(step => step.method === 'Runtime.evaluate'));
  const handle = dialog.steps.find(
    step => step.method === 'Page.handleJavaScriptDialog'
  );
  assert.deepEqual(handle.params, { accept: false });
});

test('chained flows reference earlier results by $step', () => {
  const steps = expand('css-styles', { selector: '#x' }).steps;
  const refs = JSON.stringify(steps).match(/"\$step":\d+/g);
  assert.ok(refs.length >= 3);
  for (const [index, step] of steps.entries())
    for (const [, n] of JSON.stringify(step).matchAll(/"\$step":(\d+)/g))
      assert.ok(Number(n) <= index, 'references only earlier steps');
});

test('param declarations: bounds, enum, pattern, unknown and mistyped values are refused', () => {
  const geo = def('emulate-geolocation');
  const good = { latitude: 1, longitude: 2, acknowledge: true };
  core.resolveParams(geo, good);
  assert.throws(
    () =>
      core.resolveParams(geo, {
        ...good,
        latitude: geo.params.latitude.max + 1,
      }),
    /at most/
  );
  assert.throws(
    () => core.resolveParams(def('cpu-throttle'), { rate: 0 }),
    /at least/
  );
  assert.throws(
    () => core.resolveParams(def('emulate-media'), { colorScheme: 'blue' }),
    /one of/
  );
  assert.throws(
    () =>
      core.resolveParams(def('grant-permission'), {
        permission: 'midi',
        origin: 'file:///x',
        acknowledge: true,
      }),
    /match/
  );
  assert.throws(
    () => core.resolveParams(def('cpu-throttle'), { rate: 2, extra: 1 }),
    /Unknown param/
  );
  assert.throws(
    () => core.resolveParams(def('cpu-throttle'), { rate: '2' }),
    /number/
  );
});

test('sensitive flows need acknowledge true and pass it to the cdp op', () => {
  const geo = def('emulate-geolocation');
  const base = { latitude: 1, longitude: 2 };
  assert.throws(() => core.resolveParams(geo, base), /acknowledge/);
  assert.throws(
    () => core.resolveParams(geo, { ...base, acknowledge: false }),
    /sensitive/
  );
  const plan = expand('emulate-geolocation', { ...base, acknowledge: true });
  assert.equal(plan.steps[0].acknowledge, true);
  assert.throws(
    () =>
      core.validateDefinition({
        ...geo,
        params: { ...geo.params, acknowledge: undefined },
      }),
    /acknowledge/
  );
});

test('flow run refuses a missing acknowledge or unknown param before connecting', () => {
  const missing = flow(
    'run',
    'grant-permission',
    '--params',
    '{"permission":"midi","origin":"https://a.test"}',
    '--port',
    '1'
  );
  assert.equal(missing.status, 2);
  assert.match(missing.stderr, /acknowledge/);
  assert.ok(!/ECONNREFUSED|connect/i.test(missing.stderr));
  const unknown = flow(
    'run',
    'cpu-throttle',
    '--params',
    '{"rate":2,"nope":1}',
    '--port',
    '1'
  );
  assert.equal(unknown.status, 2);
  assert.match(unknown.stderr, /Unknown param/);
});

test('flow show is spliceable and flow list groups by area and marks sensitive flows', () => {
  const show = JSON.parse(
    flow('show', 'cpu-throttle', '--params', '{"rate":4}').stdout
  );
  assert.equal(show.spliceable, true);
  assert.equal(show.valid, true);
  assert.deepEqual(show.steps, show.plan.steps);
  assert.equal(show.steps[0].params.rate, 4);
  const list = JSON.parse(flow('list').stdout);
  for (const [area, names] of Object.entries(list.areas))
    for (const name of names)
      assert.equal(
        list.flows.find(item => item.name === name).area ?? 'custom',
        area
      );
  assert.ok(list.flows.filter(item => item.sensitive).length >= 3);
  assert.ok(
    list.flows.find(item => item.name === 'emulate-geolocation').sensitive
  );
  assert.ok(
    !('sensitive' in list.flows.find(item => item.name === 'cpu-throttle'))
  );
});
