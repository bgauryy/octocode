import Ajv2020 from 'ajv/dist/2020.js';
import { expect, it } from 'vitest';
import { rustBundle } from '../scripts/tool-contract/rust.ts';
import { isObject, type Json, type JsonObject } from '../scripts/tool-contract/json.ts';

function object(value: Json | undefined): JsonObject {
  if (!isObject(value)) throw new Error('Expected a schema object');
  return value;
}

it('shares repeated anonymous string constraints without changing acceptance or use-site defaults', () => {
  const string = { type: 'string', minLength: 1, maxLength: 8, pattern: '\\S' };
  const bundle = { $defs: { Query: {
    type: 'object', additionalProperties: false,
    properties: {
      first: { ...string, description: 'First value' },
      second: { ...string, description: 'Second value', default: 'fallback' },
      different: { ...string, maxLength: 16 },
    },
    required: ['first', 'second'],
  } } };
  const projected = rustBundle(bundle);
  const defs = object(projected.$defs);
  const properties = object(object(defs.Query).properties);
  const [first, second, different] = ['first', 'second', 'different'].map(key => object(properties[key]));
  if (!first || !second || !different) throw new Error('Missing test property');
  expect(first.$ref).toBeTypeOf('string');
  expect(second.$ref).toBe(first.$ref);
  expect(second.default).toBe('fallback');
  expect(first.description).toBe('First value');
  expect(different).not.toHaveProperty('$ref', first.$ref);
  const ajv = new Ajv2020({ strict: false });
  const original = ajv.compile({ ...bundle, $ref: '#/$defs/Query' });
  const shared = ajv.compile({ ...projected, $ref: '#/$defs/Query' });
  for (const value of ['', ' ', 'a', '🙂', 'longer than eight', null, 1]) {
    const input = { first: value, second: value };
    expect(shared(input), JSON.stringify(input)).toBe(original(input));
  }
  expect(bundle.$defs.Query.properties.first).toEqual({ ...string, description: 'First value' });
});

it('preserves named definitions, literal data and schema-shaped property names', () => {
  const string = { type: 'string', minLength: 1 };
  const literal = { type: 'string', minLength: 1, description: 'Data, not a schema' };
  const bundle = { $defs: {
    Named: { ...string, title: 'Named' },
    Query: { type: 'object', properties: {
      first: string, second: string, const: string,
      literal: { const: literal, default: literal, examples: [literal] },
    } },
  } };
  const defs = object(rustBundle(bundle).$defs);
  expect(defs.Named).toEqual(bundle.$defs.Named);
  const properties = object(object(defs.Query).properties);
  expect(object(properties.const).$ref).toBe(object(properties.first).$ref);
  expect(properties.literal).toEqual(bundle.$defs.Query.properties.literal);
});

it('assigns shared types independently of property order and unrelated schemas', () => {
  const string = { type: 'string', minLength: 1 };
  const project = (properties: JsonObject) => {
    const defs = object(rustBundle({ $defs: { Query: { type: 'object', properties } } }).$defs);
    return object(object(object(defs.Query).properties).a).$ref;
  };
  expect(project({ a: string, b: string })).toBe(
    project({ z: { type: 'boolean' }, b: { minLength: 1, type: 'string' }, a: string })
  );
});
