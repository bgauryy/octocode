/**
 * The shared bulk schema defaults `format` to "tsv" — every tool that uses
 * `createRelaxedBulkQuerySchema` inherits this default, so callers who omit
 * the field get the token-efficient TSV envelope.
 */

import { describe, it, expect } from 'vitest';
import { createRelaxedBulkQuerySchema } from '../../src/scheme/localSchemaOverlay.js';
import {
  DEFAULT_TOOL_RESPONSE_FORMAT,
  type ToolExecutionArgs,
} from '../../src/types/execution.js';
import { z } from 'zod/v4';

describe('tool response format defaults', () => {
  const querySchema = z.object({ id: z.string().optional() });
  const bulkSchema = createRelaxedBulkQuerySchema('demoTool', querySchema);

  it('uses a shared default constant for MCP and CLI direct execution', () => {
    expect(DEFAULT_TOOL_RESPONSE_FORMAT).toBe('tsv');
  });

  it('MCP bulk schemas default format to "tsv" when the caller omits it', () => {
    const parsed = bulkSchema.parse({ queries: [{ id: 'q1' }] });
    expect(parsed.format).toBe('tsv');
  });

  it('MCP bulk schemas still honor an explicit format: "json" opt-out', () => {
    const parsed = bulkSchema.parse({
      queries: [{ id: 'q1' }],
      format: 'json',
    });
    expect(parsed.format).toBe('json');
  });

  it('MCP bulk schemas reject any other format value', () => {
    const result = bulkSchema.safeParse({
      queries: [{ id: 'q1' }],
      format: 'yaml',
    });
    expect(result.success).toBe(false);
  });

  it('CLI direct tool execution can use the same default format constant', () => {
    const args = {
      queries: [{ id: 'q1' }],
      format: DEFAULT_TOOL_RESPONSE_FORMAT,
    } satisfies ToolExecutionArgs<{ id: string }>;

    expect(args.format).toBe('tsv');
  });
});
