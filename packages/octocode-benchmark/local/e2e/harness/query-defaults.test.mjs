import assert from 'node:assert/strict';
import test from 'node:test';
import { createAjv, createQueryDefaults } from './query-defaults.mjs';
import { contract } from './contract.mjs';

test('canonical row schemas own defaults independently of envelope branches', () => {
  const defaults = createQueryDefaults([{
    name: 'lookup',
    querySchema: {
      $defs: { Count: { type: 'integer', default: 7 } },
      type: 'object', properties: { count: { $ref: '#/$defs/Count' } },
    },
    inputSchema: { anyOf: [{ properties: { responseSnapshot: { type: 'string' } } }] },
  }]);
  assert.deepEqual(defaults('lookup', {}), { count: 7 });
});

test('envelope fallback follows referenced alternatives and skips forbidden queries', () => {
  const inputSchema = {
    $defs: {
      Row: { type: 'object', properties: { pageSize: { type: 'integer', default: 9 } } },
      Fresh: { properties: { queries: { type: 'array', items: { $ref: '#/$defs/Row' } } } },
    },
    anyOf: [
      { $ref: '#/$defs/Fresh' },
      { properties: { queries: { not: {} }, responseSnapshot: { type: 'string' } } },
    ],
  };
  assert.deepEqual(createQueryDefaults([{ name: 'lookup', inputSchema }])('lookup', {}), { pageSize: 9 });
});

test('query defaults resolve references only in the matching operation variant', () => {
  const schema = {
    $defs: { Flag: { type: 'boolean', default: false }, Bound: { type: 'integer', default: 7 } },
    properties: { queries: { items: { anyOf: [
      { type: 'object', required: ['path', 'operation'], properties: { path: { type: 'string' }, operation: { const: 'files' }, includeHidden: { $ref: '#/$defs/Flag' }, maxFiles: { $ref: '#/$defs/Bound' }, sort: { type: 'string', default: 'path' } } },
      { type: 'object', required: ['path', 'operation'], properties: { path: { type: 'string' }, operation: { const: 'tree' }, maxDepth: { type: 'integer', default: 3 } } },
    ] } } },
  };
  const defaultsOf = createQueryDefaults([{ name: 't', inputSchema: schema }]);
  assert.deepEqual(defaultsOf('t', { path: '.', operation: 'files' }), { includeHidden: false, maxFiles: 7, sort: 'path' });
  assert.deepEqual(defaultsOf('t', { path: '.', operation: 'tree' }), { maxDepth: 3 });
  assert.deepEqual(defaultsOf('t', { path: '.', operation: 'unknown' }), {});
  assert.deepEqual(defaultsOf('unregistered', {}), {});
});

test('ambiguous schema branches do not invent a conflicting default', () => {
  const schema = { properties: { queries: { items: { anyOf: [
    { type: 'object', properties: { enabled: { type: 'boolean', default: true }, pageSize: { type: 'integer', default: 10 } } },
    { type: 'object', properties: { enabled: { type: 'boolean', default: false }, pageSize: { type: 'integer', default: 10 } } },
  ] } } } };
  const defaultsOf = createQueryDefaults([{ name: 't', inputSchema: schema }]);
  assert.deepEqual(defaultsOf('t', {}), { pageSize: 10 });
});

const deref = (root, node) => node?.$ref?.startsWith('#/') ? deref(root, node.$ref.slice(2).split('/').reduce((n, k) => n?.[k], root)) : node;

/** Every `default` a schema declares for `field`, in any variant, after resolving local refs. */
function declaredDefaults(root, field) {
  const found = [];
  const visit = (node, seen = new Set()) => {
    node = deref(root, node);
    if (!node || typeof node !== 'object' || seen.has(node)) return;
    seen.add(node);
    const property = deref(root, node.properties?.[field]);
    if (property && Object.hasOwn(property, 'default')) found.push(property.default);
    for (const value of Object.values(node)) visit(value, seen);
  };
  visit(root);
  return found;
}

test('contract defaults are declared schema defaults and agree between row and envelope schemas', () => {
  const canonical = createQueryDefaults(contract.tools);
  const envelope = createQueryDefaults(contract.tools.map(({ name, inputSchema }) => ({ name, inputSchema })));
  const ajv = createAjv({ validateFormats: false });
  const compared = [];
  for (const tool of contract.tools) {
    const schema = tool.querySchema;
    for (const [field, value] of Object.entries(canonical(tool.name, {}))) {
      assert.ok(declaredDefaults(schema, field).some(declared => JSON.stringify(declared) === JSON.stringify(value)),
        `${tool.name}.${field}=${JSON.stringify(value)} is not a declared default`);
    }
    if (schema.anyOf || schema.oneOf || schema.allOf) continue;
    const topLevel = Object.fromEntries(Object.entries(schema.properties ?? {})
      .map(([field, property]) => [field, deref(schema, property)])
      .filter(([, property]) => Object.hasOwn(property ?? {}, 'default'))
      .map(([field, property]) => [field, property.default]));
    assert.deepEqual(canonical(tool.name, {}), topLevel, tool.name);
    const query = Object.fromEntries((schema.required ?? []).map(field => [field, 'x']));
    assert.ok(ajv.validate(schema, query), `${tool.name} sample ${JSON.stringify(query)}: ${ajv.errorsText()}`);
    assert.deepEqual(envelope(tool.name, query), canonical(tool.name, query), tool.name);
    compared.push(tool.name);
  }
  assert.ok(compared.some(name => Object.keys(canonical(name, {})).length > 0), `no compared tool declares a default: ${compared.join(", ")}`);
  assert.equal(canonical('ghCloneRepo', { owner: 'bgauryy', repo: 'octocode' }).historyDepth, 1);
});
