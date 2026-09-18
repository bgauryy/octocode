import { expect } from 'vitest';
import { findDirectToolDefinition } from '@octocodeai/octocode-core/schema';

/** Check real emitted descriptors, including nested file and diagnostic pages. */
export function expectExecutableNext(value: unknown): void {
  if (!value || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    value.forEach(expectExecutableNext);
    return;
  }
  const record = value as Record<string, unknown>;
  if (
    typeof record.tool === 'string' &&
    record.query &&
    typeof record.query === 'object'
  ) {
    const definition = findDirectToolDefinition(record.tool);
    expect(definition, `Unknown next tool: ${record.tool}`).toBeDefined();
    const parsed = definition!.schema.safeParse(record.query);
    expect(
      parsed.success,
      JSON.stringify({
        next: record,
        issues: parsed.success ? [] : parsed.error.issues,
      })
    ).toBe(true);
    expect(record.query).not.toHaveProperty('goal');
    expect(record.query).toMatchObject({
      reasoning: expect.stringMatching(/\S/),
      debug: expect.any(Boolean),
    });
  }
  for (const [key, child] of Object.entries(record)) {
    if (key !== 'query') expectExecutableNext(child);
  }
}
