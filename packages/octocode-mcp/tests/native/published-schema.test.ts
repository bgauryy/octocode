import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  DIRECT_TOOL_DEFINITIONS,
  isCliOnlyTool,
} from '@octocodeai/config/schema';
import { publishedInputSchema } from '@octocodeai/config/mcp';
import { toolInputSchema } from '../../src/native/index.js';

type Standard = {
  '~standard': {
    jsonSchema: { input: (options: object) => unknown };
    validate: (value: unknown) => { value: unknown; issues?: unknown };
  };
};

describe('advertised input schema', () => {
  // CLI-only tools are never registered over MCP, so core publishes no view.
  it.each(
    DIRECT_TOOL_DEFINITIONS.map(d => d.name).filter(
      name => !isCliOnlyTool(name)
    )
  )('%s advertises core’s published view', async name => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(d => d.name === name)!;
    const schema = toolInputSchema(definition) as Standard;
    const canonical = z.toJSONSchema(definition.inputSchema, {
      io: 'input',
      unrepresentable: 'any',
    }) as Record<string, unknown>;
    expect(
      schema['~standard'].jsonSchema.input({ target: 'draft-2020-12' })
    ).toEqual(publishedInputSchema(name, canonical));
  });

  it('passes every input through: native is the only validator', () => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(
      d => d.name === 'localSearch'
    )!;
    const schema = toolInputSchema(definition) as Standard;
    const input = {
      queries: [{ matchString: 'x', path: '.', pageSize: 5000 }],
    };
    const result = schema['~standard'].validate(input);
    expect(result.issues).toBeUndefined();
    expect(result.value).toBe(input);
  });
});
