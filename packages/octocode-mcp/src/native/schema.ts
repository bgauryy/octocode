import type { getDirectToolDefinitionsWithAddons } from '@octocodeai/config/schema';
import { publishedInputSchema } from '@octocodeai/config/mcp';

export type StandardSchema = {
  '~standard': {
    version: 1;
    vendor: string;
    validate: (value: unknown) => { value: unknown };
    jsonSchema: {
      input: () => Record<string, unknown>;
      output: () => Record<string, unknown>;
    };
  };
};

export type ToolDefinition = ReturnType<
  typeof getDirectToolDefinitionsWithAddons
>[number];

/**
 * A Standard Schema that advertises `jsonSchema` in tools/list and passes
 * every input through: native normalizes and validates once, isolates invalid
 * rows, and returns the actionable repair message for a rejected call.
 */
export function passThroughSchema(
  advertised: Record<string, unknown>
): StandardSchema {
  return {
    '~standard': {
      version: 1,
      vendor: 'octocode',
      jsonSchema: { input: () => advertised, output: () => advertised },
      validate: value => ({ value }),
    },
  };
}

/** A tool's registered schema: core's slim published view of the canonical input. */
export function toolInputSchema(
  definition: Pick<ToolDefinition, 'name' | 'inputSchema'>
): StandardSchema {
  return passThroughSchema(publishedInputSchema(definition.inputSchema));
}
