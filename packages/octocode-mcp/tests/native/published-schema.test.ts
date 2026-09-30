import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';
import { publishedInputSchema } from '@octocodeai/config/mcp';
import { toolInputSchema } from '../../src/native/index.js';

type Standard = {
  '~standard': {
    jsonSchema: { input: (options: object) => unknown };
    validate: (value: unknown) => Promise<{ issues?: readonly unknown[] }>;
  };
};

describe('advertised input schema', () => {
  it.each(DIRECT_TOOL_DEFINITIONS.map(d => d.name))(
    '%s advertises core’s published view but validates canonically',
    async name => {
      const definition = DIRECT_TOOL_DEFINITIONS.find(d => d.name === name)!;
      const schema = toolInputSchema(definition) as Standard;
      const canonical = z.toJSONSchema(definition.inputSchema, {
        io: 'input',
        unrepresentable: 'any',
      }) as Record<string, unknown>;
      expect(
        schema['~standard'].jsonSchema.input({ target: 'draft-2020-12' })
      ).toEqual(publishedInputSchema(name, canonical));
    }
  );

  it('still rejects what the canonical contract rejects', async () => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(
      d => d.name === 'localSearch'
    )!;
    const schema = toolInputSchema(definition) as Standard;
    const result = await schema['~standard'].validate({
      queries: [{ searchText: 'x', path: '.', pageSize: 5000 }],
    });
    expect(result.issues?.length).toBeGreaterThan(0);
  });
});
