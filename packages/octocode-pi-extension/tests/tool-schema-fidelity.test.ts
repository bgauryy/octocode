import { expect, test } from 'vitest';
import { z } from 'zod';
import { validateToolArguments } from '@earendil-works/pi-ai';
import { toToolSchema } from '../src/tools/query-envelope.js';

test('Pi validates the same integer boundary that the owning Zod schema declares', () => {
  const schema = z.object({ offset: z.number().int() });
  const parameters = toToolSchema(schema);
  const validate = (offset: number) => validateToolArguments(
    { name: 'fixture', description: 'Fixture', parameters },
    { type: 'toolCall', id: 'integer-contract', name: 'fixture', arguments: { offset } },
  );
  expect(validate(Number.MAX_SAFE_INTEGER)).toEqual({ offset: Number.MAX_SAFE_INTEGER });
  expect(schema.safeParse({ offset: Number.MAX_SAFE_INTEGER + 1 }).success).toBe(false);
  expect(() => validate(Number.MAX_SAFE_INTEGER + 1)).toThrow();
  expect(() => validate(Number.MIN_SAFE_INTEGER - 1)).toThrow();
});

test('schema conversion preserves explicitly declared numeric bounds', () => {
  const schema = z.object({ value: z.number().max(Number.MAX_SAFE_INTEGER).min(Number.MIN_SAFE_INTEGER) });
  const { $schema: _dialect, ...expected } = z.toJSONSchema(schema, { target: 'jsonSchema7' });
  expect(toToolSchema(schema)).toEqual(expected);
});
