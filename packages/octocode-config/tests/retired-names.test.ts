import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// D1 retired-name guard: no name a hard cutover removed survives in the
// generated tool contract (input, output, or lead vocabulary).
type Retired = {
  name: string;
  scope: 'field' | 'lead' | 'input' | 'code';
  tools?: string[];
};
type Tool = {
  name: string;
  inputSchema?: unknown;
  querySchema?: unknown;
  outputSchema?: unknown;
};
type Contract = {
  tools: Tool[];
  continuationChannels: { kinds: Record<string, string[]> };
  errorCodes: Record<string, string>;
};

const load = <T>(relative: string): T =>
  JSON.parse(
    readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8')
  ) as T;

const retired = load<{ names: Retired[] }>(
  '../../../skills-dev/octocode-dev/scripts/retired-names.json'
).names;
const contract = load<Contract>('../contract/tool-contract.json');

/** Every JSON Schema property name declared anywhere under `schema`. */
function propertyNames(schema: unknown, found = new Set<string>()): Set<string> {
  if (Array.isArray(schema)) {
    for (const item of schema) propertyNames(item, found);
  } else if (schema && typeof schema === 'object') {
    for (const [key, value] of Object.entries(schema)) {
      if (key === 'properties' && value && typeof value === 'object') {
        for (const name of Object.keys(value)) found.add(name);
      }
      propertyNames(value, found);
    }
  }
  return found;
}

describe('retired names (D1 deny-list)', () => {
  it('lists well-formed entries', () => {
    const tools = new Set(contract.tools.map(tool => tool.name));
    for (const entry of retired) {
      expect(entry.name, JSON.stringify(entry)).toMatch(/^[A-Za-z][\w.]*$/);
      expect(['field', 'lead', 'input', 'code']).toContain(entry.scope);
      for (const tool of entry.tools ?? []) expect(tools).toContain(tool);
    }
  });

  it('declares only camelCase error codes, none of them retired', () => {
    const codes = Object.keys(contract.errorCodes);
    expect(codes.length).toBeGreaterThan(0);
    for (const code of codes) expect(code).toMatch(/^[a-z][A-Za-z0-9]*$/);
    for (const entry of retired.filter(entry => entry.scope === 'code')) {
      expect(codes, entry.name).not.toContain(entry.name);
    }
  });

  it.each(retired.filter(entry => entry.scope !== 'code'))(
    '$scope $name is gone from the contract',
    entry => {
      const tools = contract.tools.filter(
        tool => !entry.tools || entry.tools.includes(tool.name)
      );
      for (const tool of tools) {
        // Output rows only: embedded continuation schemas ($defs) are other
        // tools' inputs, where the same word may keep another meaning.
        const output = tool.outputSchema as
          | { properties?: Record<string, unknown> }
          | undefined;
        const schemas =
          entry.scope === 'input'
            ? [tool.inputSchema, tool.querySchema]
            : [output?.properties];
        expect(
          propertyNames(schemas).has(entry.name),
          `${tool.name} still declares ${entry.scope} ${entry.name}`
        ).toBe(false);
      }
      if (entry.scope === 'lead') {
        const kinds = Object.values(contract.continuationChannels.kinds).flat();
        expect(kinds).not.toContain(entry.name);
      }
    }
  );
});
