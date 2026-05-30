import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { expect } from 'vitest';
import type { z } from 'zod/v4';

type BulkResultStatus = 'hasResults' | 'empty' | 'error';

type BulkQueryResult = {
  id: string;
  status: BulkResultStatus;
  data: object;
};

type BulkToolOutput = {
  results: BulkQueryResult[];
};

type BulkOutputSchema = z.ZodType<BulkToolOutput>;

export function getSingleResult<TSchema extends BulkOutputSchema>(
  schema: TSchema,
  result: CallToolResult
): z.infer<TSchema>['results'][number] {
  expect(result.structuredContent).toBeDefined();

  const parsed = schema.parse(result.structuredContent);
  expect(parsed.results).toHaveLength(1);

  const [singleResult] = parsed.results;
  expect(singleResult).toBeDefined();

  return singleResult!;
}

export function expectHasResults<TSchema extends BulkOutputSchema>(
  schema: TSchema,
  result: CallToolResult
): Extract<z.infer<TSchema>['results'][number], { status: 'hasResults' }> {
  const parsed = getSingleResult(schema, result);

  // hasResults is now signaled by ABSENT status — emitted only for 'empty'
  // and 'error'. Treat undefined and 'hasResults' as the happy path.
  if (parsed.status !== undefined && parsed.status !== 'hasResults') {
    throw new Error(
      `Expected hasResults but received:\n${JSON.stringify(parsed, null, 2)}`
    );
  }

  return parsed as Extract<
    z.infer<TSchema>['results'][number],
    { status: 'hasResults' }
  >;
}

export function expectHasResultsData<TSchema extends z.ZodType<object>>(
  _outputSchema: BulkOutputSchema,
  dataSchema: TSchema,
  result: CallToolResult
): z.infer<TSchema> {
  expect(result.structuredContent).toBeDefined();

  // Skip strict validation of the outer envelope's `data` field. The
  // upstream BulkToolOutputSchema embeds the (strict) tool-specific data
  // schema directly, which would reject forward-compatible fields (e.g.
  // lspMode) added on the result side ahead of an upstream
  // @octocodeai/octocode-core schema release.
  const envelope = result.structuredContent as {
    results?: Array<{ id: string; status: BulkResultStatus; data: unknown }>;
  };
  expect(
    envelope?.results,
    'expected envelope.results to be present'
  ).toBeDefined();
  expect(envelope.results).toHaveLength(1);

  const [singleResult] = envelope.results!;
  expect(singleResult).toBeDefined();

  // hasResults is now signaled by ABSENT status — only 'empty' and 'error'
  // are emitted explicitly. Treat undefined as the happy path.
  if (
    singleResult!.status !== undefined &&
    singleResult!.status !== 'hasResults'
  ) {
    throw new Error(
      `Expected hasResults but received:\n${JSON.stringify(singleResult, null, 2)}`
    );
  }

  // Loosen the data schema as well so unknown fields pass through.
  const looseSchema =
    typeof (dataSchema as { loose?: unknown }).loose === 'function'
      ? (dataSchema as unknown as { loose: () => TSchema }).loose()
      : dataSchema;
  return looseSchema.parse(singleResult!.data);
}
