import Ajv2020 from 'ajv/dist/2020.js';
import { describe, expect, it } from 'vitest';
import { MCP_INSTRUCTION_PARTS, MCP_INSTRUCTIONS, MCP_TOOL_INSTRUCTIONS, mcpInstructions } from '@octocodeai/octocode-core/mcp';
import {
  DIRECT_TOOL_DEFINITIONS,
  getPublicToolCatalog,
  operationRows,
  publishedInputSchema,
  toolVariants,
} from '@octocodeai/octocode-core/schema';
import { at, presentation, tool, tools } from './cli-presentation-fixture.ts';
import { toolHelpSection } from '../scripts/tool-contract/presentation.ts';

const ajv = new Ajv2020({ strict: false, validateFormats: false });

function commands(help: string): Array<{ name: string; input: unknown }> {
  return [...help.matchAll(/^ {2}octocode (\w+) ('(?:[^']|'\\'')*')/gm)].map(([, name = '', quoted = '']) => ({
    name,
    input: JSON.parse(quoted.slice(1, -1).replaceAll("'\\''", "'")),
  }));
}

describe('canonical CLI and MCP discovery', () => {
  it('preserves the complete core catalog, instructions and fingerprint', () => {
    expect(presentation.instructions).toEqual(MCP_INSTRUCTION_PARTS);
    const parts = at(presentation, 'instructions');
    const routes = (Array.isArray(parts.routes) ? parts.routes : []) as Array<{ needs: string[]; text: string }>;
    const everyTool = DIRECT_TOOL_DEFINITIONS.map(entry => entry.name);
    expect(`${String(parts.head)}${routes.map(route => route.text).join('')}${String(parts.body)}`).toBe(MCP_INSTRUCTIONS);
    expect(mcpInstructions(everyTool).startsWith(MCP_INSTRUCTIONS)).toBe(true);
    expect(presentation.fingerprint).toBe(getPublicToolCatalog().fingerprint);
    expect(tools.map(entry => entry.name)).toEqual(DIRECT_TOOL_DEFINITIONS.map(entry => entry.name));
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      expect(tool(definition.name).title).toBe(definition.title);
      // A tool's block travels once, in the instruction parts.
      expect(tool(definition.name)).not.toHaveProperty('instructions');
    }
    for (const block of MCP_INSTRUCTION_PARTS.blocks) {
      expect(block.text).toBe(MCP_TOOL_INSTRUCTIONS[block.tool]);
    }
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
      const rows = operationRows(definition, false);
      expect(Object.keys(views).filter(key => !key.includes(',')).sort()).toEqual(Object.keys(rows).map(operation => `operation=${operation}`).sort());
      for (const [operation, row] of Object.entries(rows)) {
        const validate = ajv.compile(at(views, `operation=${operation}`, 'querySchema'));
        for (const variant of toolVariants(definition, false).filter(form => form.operations.includes(operation))) {
          const query = { ...variant.example, operation };
          expect(validate(query), JSON.stringify(validate.errors)).toBe(row.safeParse(query).success);
          expect(validate({ ...query, unknownField: true })).toBe(false);
        }
      }
    }
  });

  it('narrows an operation again by the second field that tells its forms apart', () => {
    const second = tools.flatMap(entry =>
      Object.keys(at(entry, 'selections'))
        .filter(key => key.includes(','))
        .map(key => ({ name: String(entry.name), key, pairs: key.split(',').map(pair => pair.split('=')) }))
    );
    expect(second.map(({ name, key }) => `${name} ${key}`)).toEqual(
      expect.arrayContaining(['astSearch operation=match,view=content', 'artifactSearch operation=resolve,ecosystem=npm'])
    );
    for (const { name, key, pairs } of second) {
      const definition = DIRECT_TOOL_DEFINITIONS.find(entry => entry.name === name);
      if (!definition) throw new Error(`No tool: ${name}`);
      const [[, operation = ''] = [], [field = '', value = ''] = []] = pairs;
      const views = at(tool(name), 'selections');
      const narrowed = at(views, key, 'querySchema');
      // The narrowed view fixes both values and is never larger than the operation's own view.
      expect(at(narrowed, 'properties', field).enum, `${name} ${key}`).toEqual([value]);
      expect(JSON.stringify(narrowed).length, `${name} ${key}`).toBeLessThanOrEqual(
        JSON.stringify(at(views, `operation=${operation}`, 'querySchema')).length
      );
      const validate = ajv.compile(narrowed);
      const row = operationRows(definition, false)[operation];
      for (const variant of toolVariants(definition, false).filter(form => form.operations.includes(operation))) {
        const query: Record<string, unknown> = { ...variant.example, operation };
        if (query[field] === undefined) continue;
        expect(validate(query), `${name} ${key}: ${JSON.stringify(query)}`).toBe(
          query[field] === value && row?.safeParse(query).success === true
        );
      }
    }
  });

  // graphQuery deps/dependents/callers take an optional view that selects
  // the grouped form; help listed "operation, target, view" with no mark, so
  // view read as required. Every form's required list stays a help line
  // (native's tool_flags guard reads it); a sub-form says what selects it.
  it('labels a sub-form by the optional field that selects it, keeping every form\'s required line', () => {
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const help = toolHelpSection(definition);
      const block = help.split('\n\n').find(text => text.startsWith('Required fields, by operation:'));
      const variants = toolVariants(definition);
      for (const variant of variants) {
        const listed = variant.requires.join(', ');
        if (listed && variants.length > 1)
          expect(
            help.split('\n').some(line => line.trimEnd().endsWith(listed)),
            `${definition.name}: ${listed}`
          ).toBe(true);
      }
      if (!block) continue;
      const rows = block
        .split('\n')
        .slice(1)
        .map(row => row.trim().split(/\s{2,}/))
        .map(([label = '', requires = '']) => ({
          label,
          operations: label.replace(/ \(with [^)]*\)$/, '').split(', '),
          requires: requires.split(', ').filter(Boolean),
        }));
      for (const row of rows)
        for (const other of rows) {
          if (other === row || other.label.includes('(with ')) continue;
          const sameOperations = row.operations.every(operation => other.operations.includes(operation));
          const fewer =
            other.requires.length < row.requires.length &&
            other.requires.every(field => row.requires.includes(field));
          if (sameOperations && fewer)
            expect(row.label, definition.name).toMatch(/ \(with [^)]+\)$/);
        }
    }
  });

  it('lists each required-fields form of a tool once, however many examples share it', () => {
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const blocks = toolHelpSection(definition).split('\n\n');
      const block = blocks.find(text => text.startsWith('Required fields, by operation:'));
      if (!block) continue;
      const rows = block.split('\n').slice(1);
      expect(new Set(rows).size, `${definition.name}: ${rows.join(' | ')}`).toBe(rows.length);
    }
  });
});
