const OPTIONAL_REASONING_SCHEMA = {
  type: 'string',
  maxLength: 400,
  description: 'Optional batch label.',
} as const;

/** Add the shared batch label without weakening item-owned required reasoning. */
export function addOptionalReasoning(schema: unknown): void {
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) return;
  const record = schema as Record<string, unknown>;
  const properties = record.properties;
  if (properties && typeof properties === 'object' && !Array.isArray(properties)) {
    const fields = properties as Record<string, unknown>;
    fields.reasoning ??= OPTIONAL_REASONING_SCHEMA;
  }
  for (const key of ['anyOf', 'oneOf', 'allOf'] as const) {
    const branches = record[key];
    if (Array.isArray(branches)) branches.forEach(addOptionalReasoning);
  }
}
