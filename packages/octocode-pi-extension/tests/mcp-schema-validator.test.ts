import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  McpSchemaUnsupportedError,
  compileMcpSchemaValidator,
} from '../src/tools/mcp-schema-validator.js';

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
