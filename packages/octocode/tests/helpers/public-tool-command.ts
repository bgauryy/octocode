const TEST_REASONING = 'Executed via octocode tool command';

function addReasoning(payload: unknown): unknown {
  if (Array.isArray(payload)) return payload.map(addReasoning);
  if (!payload || typeof payload !== 'object') return payload;
  const record = payload as Record<string, unknown>;
  if (Array.isArray(record.queries)) {
    return { ...record, queries: record.queries.map(addReasoning) };
  }
  return { reasoning: TEST_REASONING, ...record };
}

/** Supply caller-owned reasoning to CLI behavior tests unrelated to validation. */
export async function invokePublicToolCommand(
  command: { handler?: (args: never) => unknown },
  args: Record<string, unknown> & {
    options?: Record<string, unknown>;
    raw?: unknown[];
  }
): Promise<unknown> {
  const options = { ...(args.options ?? {}) };
  if (typeof options.queries === 'string') {
    try {
      options.queries = JSON.stringify(
        addReasoning(JSON.parse(options.queries))
      );
    } catch {
      // Keep malformed JSON unchanged so parsing-error tests retain their subject.
    }
  } else if (options.queries !== undefined) {
    options.queries = addReasoning(options.queries);
  } else {
    const raw = Array.isArray(args.raw) ? [...args.raw] : undefined;
    const hasRawFieldInput = raw?.some(
      (value: unknown, index: number) =>
        index > 1 && typeof value === 'string' && value.startsWith('--')
    );
    if (hasRawFieldInput && raw) {
      raw.push('--reasoning', TEST_REASONING);
      args = { ...args, raw };
    }
    const outputOnly = new Set([
      'compact',
      'json',
      'yaml',
      'scheme',
      'help',
      'pretty',
    ]);
    const hasFieldInput = Object.entries(options).some(
      ([key, value]) => !outputOnly.has(key) && value !== undefined
    );
    if (!hasRawFieldInput && hasFieldInput)
      options.reasoning ??= TEST_REASONING;
  }
  const handler = command.handler as unknown as (
    value: Record<string, unknown>
  ) => unknown;
  return handler({ ...args, options });
}
