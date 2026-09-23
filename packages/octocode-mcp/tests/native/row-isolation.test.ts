import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';
import { rowIsolatingSchema, wrapBareQuery } from '../../src/native/index.js';

type Standard = Parameters<typeof rowIsolatingSchema>[0];

const definition = DIRECT_TOOL_DEFINITIONS.find(
  tool => tool.name === 'localSearch'
)!;
const canonical = z.preprocess(
  wrapBareQuery,
  definition.inputSchema
) as unknown as Standard;
const schema = rowIsolatingSchema(
  canonical,
  definition.schema,
  definition.inputSchema
);
const row = (extra: Record<string, unknown> = {}) => ({
  path: '/repo',
  searchText: 'needle',
  reasoning: 'Find needles.',
  ...extra,
});
const validate = (value: unknown) =>
  Promise.resolve(schema['~standard'].validate(value));

describe('MCP batch row isolation', () => {
  it('advertises exactly the canonical input schema', () => {
    expect(
      (
        schema['~standard'].jsonSchema as { input: (o: object) => unknown }
      ).input({ target: 'draft-2020-12' })
    ).toEqual(
      z.toJSONSchema(definition.inputSchema, {
        target: 'draft-2020-12',
        io: 'input',
      })
    );
  });

  it('passes a batch with at least one valid row through to native', async () => {
    const input = { queries: [row(), row({ serchText: 'typo' })] };
    const result = await validate(input);
    expect(result.issues).toBeUndefined();
    expect((result as { value: unknown }).value).toEqual(input);
  });

  it('keeps whole-batch failures for all-invalid, envelope, and single-row input', async () => {
    for (const input of [
      { queries: [row({ bogus: 1 }), row({ alsoBogus: 2 })] },
      { queries: Array.from({ length: 6 }, () => row()) },
      { queries: [row({ bogus: 1 })] },
    ])
      expect((await validate(input)).issues?.length).toBeGreaterThan(0);
  });

  it('still wraps a bare query before validation', async () => {
    const result = await validate(row());
    expect(result.issues).toBeUndefined();
  });
});
