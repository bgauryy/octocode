// Dynamic CDP: describe, search and validate from a schema document, with a per-version cache.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, readdirSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  loadSchema,
  clearSchemaMemo,
  describeMethod,
  describeMember,
  describeDomain,
  lookupMember,
  searchSchema,
  validateParams,
  validationRefusal,
  nearestMethods,
  gateCdpCall,
} from '../../dist/engine/cdp-schema.mjs';

const schema = {
  domains: [
    {
      domain: 'Thing',
      description: 'Things. Second sentence.',
      types: [
        { id: 'Id', type: 'integer' },
        { id: 'Mode', type: 'string', enum: ['fast', 'slow'] },
        {
          id: 'Box',
          type: 'object',
          properties: [
            { name: 'x', type: 'number' },
            { name: 'mode', $ref: 'Mode', optional: true },
          ],
        },
      ],
      commands: [
        {
          name: 'find',
          description: 'Finds a thing by selector.',
          parameters: [
            { name: 'id', $ref: 'Id' },
            { name: 'selector', type: 'string' },
            { name: 'flag', type: 'boolean', optional: true },
            {
              name: 'boxes',
              type: 'array',
              items: { $ref: 'Box' },
              optional: true,
            },
            { name: 'level', type: 'string', enum: ['a', 'b'], optional: true },
            { name: 'any', type: 'any', optional: true },
          ],
          returns: [{ name: 'found', type: 'boolean' }],
          experimental: true,
        },
        { name: 'list', description: 'Lists geolocation things.' },
      ],
      events: [{ name: 'found', description: 'A thing was found.' }],
    },
  ],
};

test('describeMethod is compact and links the docs', () => {
  const d = describeMethod(schema, 'Thing.find');
  assert.equal(d.domain, 'Thing');
  assert.deepEqual(
    d.params.map(p => [p.name, p.type, p.optional]),
    [
      ['id', 'Thing.Id', false],
      ['selector', 'string', false],
      ['flag', 'boolean', true],
      ['boxes', 'array<Thing.Box>', true],
      ['level', 'string', true],
      ['any', 'any', true],
    ]
  );
  assert.deepEqual(d.params[4].enum, ['a', 'b']);
  assert.equal(d.experimental, true);
  assert.equal(d.policy, 'free');
  assert.match(d.docsUrl, /\/tot\/Thing\/#method-find$/);
  assert.match(describeMember(schema, 'Thing.found').docsUrl, /#event-found$/);
  assert.equal(lookupMember(schema, 'Thing').kind, 'domain');
  assert.equal(
    describeDomain(lookupMember(schema, 'Thing').domain).methods.length,
    2
  );
});

test('validateParams checks required, types, enums, $ref, arrays and unknown names', () => {
  const ok = { id: 1, selector: 'a' };
  assert.deepEqual(validateParams(schema, 'Thing.find', ok), { ok: true });
  const bad = (params, pattern) => {
    const r = validateParams(schema, 'Thing.find', params);
    assert.equal(r.ok, false);
    assert.match(r.errors.join('\n'), pattern);
    assert.equal(r.expected.name, 'Thing.find');
  };
  bad({ selector: 'a' }, /params\.id: required/);
  bad({ id: 'x', selector: 'a' }, /params\.id: expected integer, got string/);
  bad({ id: 1.5, selector: 'a' }, /expected integer/);
  bad({ ...ok, flag: 'yes' }, /params\.flag: expected boolean/);
  bad({ ...ok, level: 'c' }, /not one of a, b/);
  bad({ ...ok, extra: 1 }, /params\.extra: unknown parameter/);
  bad({ ...ok, boxes: 'x' }, /expected array/);
  bad(
    { ...ok, boxes: [{ x: 1 }, { mode: 'fast' }] },
    /boxes\[1\]\.x: required/
  );
  bad({ ...ok, boxes: [{ x: 1, mode: 'wrong' }] }, /boxes\[0\]\.mode/);
  assert.equal(
    validateParams(schema, 'Thing.find', {
      ...ok,
      boxes: [{ x: 1, mode: 'slow' }],
    }).ok,
    true
  );
  assert.match(
    validationRefusal('Thing.find', validateParams(schema, 'Thing.find', {})),
    /Docs: .*#method-find/
  );
});

test('$step references are not type-checked but the rest is', () => {
  assert.equal(
    validateParams(schema, 'Thing.find', {
      id: { $step: 1, pointer: '/id' },
      selector: 'a',
    }).ok,
    true
  );
  const r = validateParams(schema, 'Thing.find', {
    id: { $step: 1 },
    selector: 3,
  });
  assert.equal(r.ok, false);
  assert.doesNotMatch(r.errors.join(), /params\.id/);
});

test('an unknown method gets the nearest names', () => {
  assert.equal(nearestMethods(schema, 'Thing.fnd')[0], 'Thing.find');
  const r = validateParams(schema, 'Thing.fnd', {});
  assert.equal(r.ok, false);
  assert.equal(r.nearest[0], 'Thing.find');
  assert.match(validationRefusal('Thing.fnd', r), /Did you mean: Thing\.find/);
});

test('search is case-insensitive, ranked and pages without dropping hits', () => {
  const all = searchSchema(schema, 'FIND');
  assert.equal(all.hits[0].name, 'Thing.find');
  assert.ok(!all.hits.some(h => h.name === 'Thing.found') || all.total >= 1);
  const geo = searchSchema(schema, 'geolocation');
  assert.deepEqual(
    geo.hits.map(h => h.name),
    ['Thing.list']
  );
  const pages = [];
  for (let offset = 0; ;) {
    const page = searchSchema(schema, 't', { offset, limit: 2 });
    pages.push(...page.hits.map(h => h.name));
    if (page.nextOffset === undefined) break;
    offset = page.nextOffset;
  }
  assert.equal(pages.length, searchSchema(schema, 't').total);
  assert.equal(new Set(pages).size, pages.length);
});

test('loadSchema caches per browser version in a 0600 file and rereads it', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'octo-schema-'));
  const calls = [];
  const fetcher = browser => async path => {
    calls.push(path);
    return path === '/json/version' ? { Browser: browser } : schema;
  };
  try {
    clearSchemaMemo();
    await loadSchema(9, { cacheDir: dir, fetchJson: fetcher('Chrome/1.0') });
    assert.equal(calls.filter(p => p === '/json/protocol').length, 1);
    const [file] = readdirSync(dir);
    assert.equal(statSync(join(dir, file)).mode & 0o777, 0o600);
    clearSchemaMemo(); // a new process: only the file remains
    const again = await loadSchema(9, {
      cacheDir: dir,
      fetchJson: fetcher('Chrome/1.0'),
    });
    assert.equal(calls.filter(p => p === '/json/protocol').length, 1);
    assert.equal(again.domains[0].domain, 'Thing');
    await loadSchema(9, { cacheDir: dir, fetchJson: fetcher('Chrome/2.0') });
    assert.equal(calls.filter(p => p === '/json/protocol').length, 2);
    assert.equal(readdirSync(dir).length, 2);
    await assert.rejects(loadSchema(0, { cacheDir: dir }), /Invalid port/);
  } finally {
    clearSchemaMemo();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('gateCdpCall refuses by policy and schema, and never blocks when the schema is unavailable', async () => {
  const warnings = [];
  const unavailable = {
    cacheDir: tmpdir(),
    fetchJson: async () => {
      throw new Error('offline');
    },
  };
  assert.match(
    await gateCdpCall(
      { method: 'Browser.close', params: {} },
      9,
      () => {},
      unavailable
    ),
    /blocked/
  );
  assert.equal(
    await gateCdpCall(
      { method: 'DOM.nothing', params: {} },
      9,
      l => warnings.push(l),
      unavailable
    ),
    undefined
  );
  assert.match(warnings[0], /not validated/);
  clearSchemaMemo();
  const dir = mkdtempSync(join(tmpdir(), 'octo-gate-'));
  const live = {
    cacheDir: dir,
    fetchJson: async p => (p === '/json/version' ? { Browser: 'G/1' } : schema),
  };
  try {
    const warn = [];
    assert.equal(
      await gateCdpCall(
        { method: 'Thing.find', params: { id: 1, selector: 's' } },
        9,
        l => warn.push(l),
        live
      ),
      undefined
    );
    assert.match(warn[0], /\[WARNING\].*experimental/);
    assert.match(
      await gateCdpCall(
        { method: 'Thing.find', params: { id: 'x', selector: 's' } },
        9,
        () => {},
        live
      ),
      /\[INVALID\]/
    );
  } finally {
    clearSchemaMemo();
    rmSync(dir, { recursive: true, force: true });
  }
});
