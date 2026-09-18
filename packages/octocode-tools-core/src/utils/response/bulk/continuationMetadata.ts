export function preserveContinuationMetadata(
  value: unknown,
  originalQuery: Readonly<Record<string, unknown>>
): unknown {
  if (Array.isArray(value)) {
    return value.map(item => preserveContinuationMetadata(item, originalQuery));
  }
  if (value === null || typeof value !== 'object') return value;

  const record = value as Record<string, unknown>;
  const mapped = Object.fromEntries(
    Object.entries(record).map(([key, child]) => [
      key,
      preserveContinuationMetadata(child, originalQuery),
    ])
  );
  if (
    typeof record.tool === 'string' &&
    record.query !== null &&
    typeof record.query === 'object' &&
    !Array.isArray(record.query)
  ) {
    mapped.query = {
      ...(mapped.query as Record<string, unknown>),
      ...(typeof originalQuery.reasoning === 'string'
        ? { reasoning: originalQuery.reasoning }
        : {}),
      ...(typeof originalQuery.debug === 'boolean'
        ? { debug: originalQuery.debug }
        : {}),
    };
  }
  return mapped;
}
