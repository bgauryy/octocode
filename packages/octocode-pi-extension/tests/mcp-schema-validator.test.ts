import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  McpSchemaUnsupportedError,
  compileMcpSchemaValidator,
} from '../src/tools/mcp/schema-validator.js';

test('validator accepts representative valid MCP arguments across required JSON Schema constructs', () => {
  const schemas: Array<[unknown, unknown]> = [
    [{
      type: 'object',
      required: ['queries'],
      additionalProperties: false,
      properties: {
        queries: {
          type: 'array',
          minItems: 1,
          items: {
            type: 'object',
            required: ['path'],
            properties: { path: { type: 'string', minLength: 1 } },
          },
        },
      },
    }, { queries: [{ path: '/tmp/file.ts' }] }],
    [{ anyOf: [{ type: 'string' }, { type: 'number' }] }, 42],
    [{ oneOf: [{ type: 'string' }, { type: 'number' }] }, 'value'],
    [{ allOf: [{ type: 'number' }, { minimum: 0 }] }, 1],
    [{ $defs: { value: { type: 'string', minLength: 1 } }, $ref: '#/$defs/value' }, 'value'],
    [{ type: ['string', 'null'] }, null],
    [{ enum: ['a', 'b'] }, 'a'],
  ];

  for (const [schema, value] of schemas) {
    assert.deepEqual(compileMcpSchemaValidator(schema).validate(value), { valid: true, errors: [] });
  }
});

test('validator returns bounded path-specific errors for malformed arguments', () => {
  const validator = compileMcpSchemaValidator({
    type: 'object',
    required: ['queries'],
    additionalProperties: false,
    properties: {
      queries: {
        type: 'array',
        minItems: 1,
        items: {
          type: 'object',
          required: ['path'],
          additionalProperties: false,
          properties: { path: { type: 'string', minLength: 2 } },
        },
      },
    },
  });
  const result = validator.validate({ queries: [{ path: '', extra: true }], extra: true });

  assert.equal(result.valid, false);
  assert.ok(result.errors.length > 0 && result.errors.length <= 8);
  assert.ok(result.errors.some((error) => error.instancePath === '/queries/0/path'));
  assert.ok(result.errors.every((error) => error.message.length <= 240));
});

test('validator rejects unsupported dialects and oversized schemas before validation', () => {
  assert.throws(
    () => compileMcpSchemaValidator({ $schema: 'https://example.com/custom-schema', type: 'object' }),
    McpSchemaUnsupportedError,
  );
  assert.throws(
    () => compileMcpSchemaValidator({ type: 'string', description: 'x'.repeat(300_000) }),
    /too large|unsupported/i,
  );
});

test('legacy dialects are rejected instead of silently weakening their constraints', () => {
  for (const schema of [
    { $schema: 'http://json-schema.org/draft-04/schema#', type: 'number', minimum: 5, exclusiveMinimum: true },
    { $schema: 'http://json-schema.org/draft-03/schema#', type: 'object', properties: { name: { type: 'string', required: true } } },
    ...['06', '07'].map(draft => ({
      $schema: `http://json-schema.org/draft-${draft}/schema#`,
      definitions: { base: { type: 'number' } },
      $ref: '#/definitions/base',
      minimum: 10,
    })),
  ]) {
    assert.throws(() => compileMcpSchemaValidator(schema), McpSchemaUnsupportedError);
  }
  const modern = compileMcpSchemaValidator({
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    type: 'number',
    exclusiveMinimum: 5,
  });
  assert.equal(modern.validate(5).valid, false);
  assert.equal(modern.validate(6).valid, true);
});

test('modern dialect constructs retain tuple, dependency, and unevaluated-property validation', () => {
  const cases: Array<[unknown, unknown, unknown]> = [
    [{ $schema: 'https://json-schema.org/draft/2020-12/schema', type: 'array', prefixItems: [{ type: 'string' }, { type: 'number' }], items: false }, ['ok', 3], ['ok', 'bad']],
    [{ $schema: 'https://json-schema.org/draft/2019-09/schema', type: 'object', dependentRequired: { a: ['b'] } }, { a: 1, b: 2 }, { a: 1 }],
    [{ $schema: 'https://json-schema.org/draft/2019-09/schema', type: 'object', allOf: [{ properties: { a: { type: 'string' } } }], unevaluatedProperties: false }, { a: 'ok' }, { a: 'ok', b: 3 }],
  ];
  for (const [schema, valid, invalid] of cases) {
    const validator = compileMcpSchemaValidator(schema);
    assert.equal(validator.validate(valid).valid, true);
    assert.equal(validator.validate(invalid).valid, false);
  }
});

test('union diagnostics report the selected operation before applying the error limit', () => {
  const branch = (operation: string, properties: Record<string, unknown>, required: string[] = []) => ({
    type: 'object',
    required: ['operation', 'path', ...required],
    additionalProperties: false,
    properties: { operation: { const: operation }, path: { type: 'string' }, ...properties },
  });
  const validator = compileMcpSchemaValidator({
    type: 'object',
    properties: {
      queries: {
        type: 'array',
        items: {
          anyOf: [
            branch('text', { searchText: { type: 'string' } }, ['searchText']),
            branch('structural', { pattern: { type: 'string' } }, ['pattern']),
            branch('structural', { rule: { type: 'string' } }, ['rule']),
            branch('files', { names: { type: 'array', items: { type: 'string' } } }),
            branch('tree', { namePattern: { type: 'string' } }),
          ],
        },
      },
    },
  });
  for (const operation of ['files', 'tree']) {
    const result = validator.validate({ queries: [{ operation, path: '/repo', searchText: 'bad-field' }] });
    assert.equal(result.valid, false);
    assert.ok(result.errors.some((error) => error.instancePath === '/queries/0/searchText'));
    assert.ok(result.errors.every((error) => !/anyOf\/[012](?:\/|$)/.test(error.schemaPath)), JSON.stringify(result.errors));
    assert.ok(result.errors.length <= 8);
  }
  const unknown = validator.validate({ queries: [{ operation: 'unknown', path: '/repo' }] });
  assert.equal(unknown.valid, false);
  assert.ok(unknown.errors.some((error) => error.instancePath === '/queries/0/operation'));

  const structural = validator.validate({ queries: [{ operation: 'structural', path: '/repo' }] });
  assert.equal(structural.valid, false);
  assert.ok(structural.errors.some((error) => error.message.includes('pattern')));
  assert.ok(structural.errors.some((error) => error.message.includes('rule')));
  assert.ok(structural.errors.every((error) => !/anyOf\/[034](?:\/|$)/.test(error.schemaPath)));

  const mixed = validator.validate({ queries: [
    { operation: 'files', path: '/repo', searchText: 'bad-field' },
    { operation: 'tree', path: '/repo', searchText: 'bad-field' },
  ] });
  assert.equal(mixed.valid, false);
  assert.ok(mixed.errors.some((error) => error.instancePath === '/queries/0/searchText' && error.schemaPath.includes('/anyOf/3/')));
  assert.ok(mixed.errors.some((error) => error.instancePath === '/queries/1/searchText' && error.schemaPath.includes('/anyOf/4/')));
  assert.equal(validator.validate({ queries: [{ operation: 'files', path: '/repo' }] }).valid, true);
  assert.equal(validator.validate({ queries: [{ operation: 'files', path: '/repo', names: 'not-an-array' }] }).valid, false);
});

test('oneOf diagnostics preserve the selected branch required field and TypeBox instance path', () => {
  const validator = compileMcpSchemaValidator({
    type: 'object',
    properties: {
      'a/b': {
        oneOf: ['first', 'second', 'third'].map((kind) => ({
          type: 'object',
          properties: { kind: { const: kind }, value: { type: 'string' } },
          required: ['kind', 'value'],
        })),
      },
    },
  });
  const result = validator.validate({ 'a/b': { kind: 'third' } });
  assert.equal(result.valid, false);
  assert.ok(result.errors.some((error) => error.instancePath === '/a/b' && error.message.includes('value')));
  assert.ok(result.errors.every((error) => !/oneOf\/[01](?:\/|$)/.test(error.schemaPath)));
});
