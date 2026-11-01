import Ajv2020 from 'ajv/dist/2020.js';
import { describe, expect, it } from 'vitest';
import { MCP_INSTRUCTIONS } from '@octocodeai/octocode-core/mcp';
import {
  DIRECT_TOOL_DEFINITIONS,
  getPublicToolCatalog,
  operationRows,
  publishedInputSchema,
  toolVariants,
} from '@octocodeai/octocode-core/schema';
import { at, presentation, tool, tools } from './cli-presentation-fixture.ts';

const ajv = new Ajv2020({ strict: false, validateFormats: false });

function commands(help: string): Array<{ name: string; input: unknown }> {
  return [...help.matchAll(/^ {2}octocode (\w+) ('(?:[^']|'\\'')*')/gm)].map(([, name = '', quoted = '']) => ({
    name,
    input: JSON.parse(quoted.slice(1, -1).replaceAll("'\\''", "'")),
  }));
}

describe('canonical CLI and MCP discovery', () => {
  it('preserves the complete core catalog, instructions and fingerprint', () => {
    expect(presentation.instructions).toBe(MCP_INSTRUCTIONS);
    expect(presentation.fingerprint).toBe(getPublicToolCatalog().fingerprint);
    expect(tools.map(entry => entry.name)).toEqual(DIRECT_TOOL_DEFINITIONS.map(entry => entry.name));
    for (const definition of DIRECT_TOOL_DEFINITIONS)
      expect(tool(definition.name).title).toBe(definition.title);
  });

  it('publishes the same request envelope in CLI schema and MCP', () => {
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const entry = tool(definition.name);
      const expected = publishedInputSchema(definition);
      expect(at(entry, 'mcp').inputSchema).toEqual(expected);
      expect(at(entry, 'views', 'query').inputSchema).toEqual(expected);
      const validate = ajv.compile(expected);
      for (const variant of toolVariants(definition)) {
        if (!variant.example) throw new Error(`Missing example for ${definition.name}`);
        const input = { queries: [variant.example] };
        expect(validate(input), JSON.stringify(validate.errors)).toBe(true);
        // A cursor beside queries is native's to reject: the listing is one object with each field once.
        for (const rejected of [variant.example, { ...input, responseLength: 1000 }, { queries: [{ ...variant.example, extras: ['hints'] }] }])
          expect(validate(rejected), JSON.stringify(rejected)).toBe(false);
      }
      expect(validate({ cursor: 'returned-token' })).toBe(true);
    }
  });

  it('makes every help example an executable canonical JSON request', () => {
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const entry = tool(definition.name);
      const validate = ajv.compile(at(entry, 'mcp', 'inputSchema'));
      const examples = commands(String(entry.help));
      expect(examples.length, definition.name).toBeGreaterThan(0);
      for (const example of examples) {
        expect(example.name).toBe(definition.name);
        expect(validate(example.input), JSON.stringify(validate.errors)).toBe(true);
      }
      expect(entry).not.toHaveProperty('cli');
    }
  });

  it('keeps every operation discoverable with its own admission constraints', () => {
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const views = at(tool(definition.name), 'selections');
      expect(Object.keys(views).sort()).toEqual(Object.keys(operationRows(definition)).map(operation => `operation=${operation}`).sort());
      for (const [operation, row] of Object.entries(operationRows(definition))) {
        const validate = ajv.compile(at(views, `operation=${operation}`, 'querySchema'));
        for (const variant of toolVariants(definition).filter(form => form.operations.includes(operation))) {
          const query = { ...variant.example, operation };
          expect(validate(query), JSON.stringify(validate.errors)).toBe(row.safeParse(query).success);
          expect(validate({ ...query, unknownField: true })).toBe(false);
        }
      }
    }
  });
});
