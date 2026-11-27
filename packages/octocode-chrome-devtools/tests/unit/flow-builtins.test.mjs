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
const { INLINE_STDOUT_BYTES } = await import(join(dist, 'capture-result.mjs'));
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
  const evaluate = dialog.steps.find(step => step.op === 'evaluate');
  assert.equal(typeof evaluate.expression, 'string');
  assert.match(evaluate.expression, /alert\(1\)/);
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
  assert.ok(!dialog.steps.some(step => step.op === 'evaluate'));
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

test('flow show returns the definition once and is spliceable', () => {
  const show = JSON.parse(
    flow('show', 'cpu-throttle', '--params', '{"rate":4}').stdout
  );
  assert.deepEqual(Object.keys(show).sort(), [
    'area',
    'description',
    'expects',
    'missing',
    'name',
    'params',
    'source',
    'spliceable',
    'steps',
    'valid',
  ]);
  assert.equal(show.spliceable, true);
  assert.equal(show.valid, true);
  assert.deepEqual(show.missing, []);
  assert.equal(show.steps[0].params.rate, 4);
});

test('flow show is not valid while a required param is missing or a sensitive flow is unconfirmed', () => {
  const bare = JSON.parse(flow('show', 'emulate-device').stdout);
  assert.equal(bare.valid, false);
  assert.deepEqual(bare.missing.sort(), ['height', 'width']);
  const partial = JSON.parse(
    flow('show', 'emulate-device', '--params', '{"width":300}').stdout
  );
  assert.equal(partial.valid, false);
  assert.deepEqual(partial.missing, ['height']);
  const full = JSON.parse(
    flow('show', 'emulate-device', '--params', '{"width":300,"height":500}')
      .stdout
  );
  assert.equal(full.valid, true);
  const geo = '{"latitude":1,"longitude":2,"acknowledge":false}';
  const unconfirmed = JSON.parse(
    flow('show', 'emulate-geolocation', '--params', geo).stdout
  );
  assert.equal(unconfirmed.valid, false);
  assert.ok(unconfirmed.missing.includes('acknowledge'));
  const confirmed = JSON.parse(
    flow(
      'show',
      'emulate-geolocation',
      '--params',
      geo.replace('false', 'true')
    ).stdout
  );
  assert.equal(confirmed.valid, true);
});

test('flow list is one compact inline reply grouped by area', () => {
  const out = flow('list').stdout;
  const list = JSON.parse(out);
  assert.equal(out.trim().split('\n').length, 1, 'one line');
  assert.ok(
    Buffer.byteLength(out) <= INLINE_STDOUT_BYTES,
    `${Buffer.byteLength(out)} bytes exceed the inline limit ${INLINE_STDOUT_BYTES}`
  );
  const rows = Object.values(list.areas).flat();
  assert.equal(rows.length, builtinFlows.length);
  for (const [area, items] of Object.entries(list.areas))
    for (const item of items) {
      const source = def(item.name);
      assert.equal(source.area, area);
      assert.deepEqual(
        Object.keys(item).filter(key => key !== 'sensitive'),
        ['name', 'description']
      );
      assert.equal(item.sensitive, source.sensitive);
      assert.ok(item.description.length <= core.LIST_DESCRIPTION_MAX);
      assert.ok(!item.description.endsWith('…'), `${item.name} is cut`);
      assert.ok(source.description.startsWith(item.description));
      assert.ok(!('params' in item) && !('expects' in item));
    }
  assert.match(list.next, /flow show/);
});

test('firstSentence keeps whole sentences and cuts long ones at a word', () => {
  assert.equal(core.firstSentence('One. Two.'), 'One.');
  assert.equal(core.firstSentence('No period here'), 'No period here');
  const long = `${'word '.repeat(40)}end.`;
  const cut = core.firstSentence(long);
  assert.ok(cut.length <= core.LIST_DESCRIPTION_MAX && cut.endsWith('…'));
});

test('the data flows exist, validate, and their sensitive ones refuse a missing or false acknowledge', () => {
  const names = [
    'dom-snapshot',
    'indexeddb-dump',
    'cache-storage-dump',
    'service-worker-state',
    'network-mock',
    'block-urls',
    'system-info',
    'storage-quota',
  ];
  for (const name of names) {
    assert.ok(def(name), name);
    validatePlan(core.expandFlow(def(name), core.sampleParams(def(name))).plan);
  }
  for (const name of [
    'indexeddb-dump',
    'cache-storage-dump',
    'network-mock',
    'block-urls',
  ]) {
    assert.equal(def(name).sensitive, true, name);
    const sample = core.sampleParams(def(name));
    assert.throws(
      () => core.resolveParams(def(name), { ...sample, acknowledge: false }),
      /sensitive/,
      name
    );
    // Every step that Chrome's policy gates carries the acknowledge.
    for (const step of core.expandFlow(def(name), sample).plan.steps)
      if (
        step.op === 'cdp' &&
        step.method !== 'Network.enable' &&
        step.method !== 'Fetch.disable'
      )
        assert.equal(step.acknowledge, true, `${name}: ${step.method}`);
  }
  for (const name of [
    'system-info',
    'service-worker-state',
    'dom-snapshot',
    'storage-quota',
  ])
    assert.ok(!def(name).sensitive, name);
});

test('data flow steps are typed and spliceable: saved names, typed numbers, absent optionals dropped', () => {
  const snapshot = expand('dom-snapshot', {});
  assert.deepEqual(snapshot.steps[0].params.computedStyles, ['display']);
  assert.equal(snapshot.steps[0].save, 'dom-snapshot.json');
  assert.equal(expand('system-info', {}).steps[0].save, 'system-info.json');
  const quota = expand('storage-quota', { origin: 'https://example.com' });
  assert.equal(quota.steps[0].params.origin, 'https://example.com');
  assert.throws(
    () => core.resolveParams(def('storage-quota'), { origin: 'file:///x' }),
    /match/
  );
  const idb = expand('indexeddb-dump', {
    origin: 'https://example.com',
    acknowledge: true,
  });
  const data = idb.steps.find(step => step.method === 'IndexedDB.requestData');
  assert.equal(typeof data.params.pageSize, 'number');
  const chained = JSON.stringify(idb.steps).match(/"\$step":\d+/g);
  assert.ok(chained.length >= 2);
  const mock = expand('network-mock', {
    pattern: '*://*/api',
    acknowledge: true,
  });
  assert.ok(
    !mock.steps.some(step => step.op === 'evaluate'),
    'no trigger, no evaluate step'
  );
  const fulfil = mock.steps.find(
    step => step.method === 'Fetch.fulfillRequest'
  );
  assert.equal(fulfil.params.responseCode, 200);
  assert.ok(!('body' in fulfil.params), 'absent body is dropped');
  assert.deepEqual(fulfil.params.requestId, {
    $event: 'paused',
    pointer: '/requestId',
  });
  const withBody = expand('network-mock', {
    pattern: 'x',
    body: 'aGk=',
    trigger: 'fetch("/a")',
    acknowledge: true,
  });
  assert.equal(
    withBody.steps.find(step => step.method === 'Fetch.fulfillRequest').params
      .body,
    'aGk='
  );
  assert.ok(withBody.steps.some(step => step.op === 'evaluate'));
});

test('a time zone needs acknowledge (Emulation.setTimezoneOverride is gated); a locale alone does not', () => {
  const zone = expand('emulate-timezone-locale', {
    timezone: 'Asia/Tokyo',
    acknowledge: true,
  });
  assert.equal(zone.steps[0].acknowledge, true);
  assert.throws(
    () => expand('emulate-timezone-locale', { timezone: 'Asia/Tokyo' }),
    /needs acknowledge together with timezone/
  );
  const locale = expand('emulate-timezone-locale', { locale: 'de-DE' });
  assert.equal(locale.steps.length, 1);
  assert.equal(locale.steps[0].method, 'Emulation.setLocaleOverride');
});
