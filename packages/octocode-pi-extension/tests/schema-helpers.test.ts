/**
 * TDD tests for schema-helpers.ts
 */
import assert from 'node:assert/strict';
import { test } from 'vitest';
import { Type } from 'typebox';
import { stringEnumSchema } from '../src/tools/schema-helpers.js';

test('stringEnumSchema returns a TypeBox Unsafe schema with type:string', () => {
  const schema = stringEnumSchema(Type, ['foo', 'bar', 'baz'], 'A test enum');
  assert.equal((schema as { type?: string }).type, 'string');
});

test('stringEnumSchema includes all provided enum values', () => {
  const schema = stringEnumSchema(Type, ['a', 'b', 'c'], 'values');
  assert.deepEqual((schema as { enum?: string[] }).enum, ['a', 'b', 'c']);
});

test('stringEnumSchema includes the description', () => {
  const schema = stringEnumSchema(Type, ['x'], 'My description');
  assert.equal((schema as { description?: string }).description, 'My description');
});

test('stringEnumSchema does not mutate the original values array', () => {
  const values = ['a', 'b'] as const;
  const schema = stringEnumSchema(Type, values, 'copy test');
  const enumArr = (schema as { enum?: string[] }).enum ?? [];
  enumArr.push('c');
  assert.equal(values.length, 2, 'original readonly array must not be mutated');
});

test('stringEnumSchema handles an empty values array', () => {
  const schema = stringEnumSchema(Type, [] as const, 'empty');
  assert.deepEqual((schema as { enum?: string[] }).enum, []);
});

test('stringEnumSchema handles a single-element values array', () => {
  const schema = stringEnumSchema(Type, ['only'] as const, 'singleton');
  assert.deepEqual((schema as { enum?: string[] }).enum, ['only']);
});
