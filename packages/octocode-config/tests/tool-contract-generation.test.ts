import Ajv2020, { type AnySchema } from 'ajv/dist/2020.js';
import { rustBundle } from '../scripts/tool-contract/rust.ts';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { buildEnforcementContractIr } from '@octocodeai/octocode-core/schema';
import { buildToolTypesBundle } from '../scripts/generate-tool-contract.ts';

/** An object row of string properties whose first key is required. */
const stringRow = (...keys: string[]) => ({
  type: 'object',
  properties: Object.fromEntries(keys.map(key => [key, { type: 'string' }])),
  required: keys.slice(0, 1),
});

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string): string =>
  readFileSync(resolve(packageRoot, path), 'utf8');

// The IR and bundle are pure functions of core's tool definitions and cost seconds to build:
// build them once for every check that reads them. Checks that need a variant copy them.
let ir: ReturnType<typeof buildEnforcementContractIr>;
let built: ReturnType<typeof buildToolTypesBundle>;
beforeAll(() => {
  ir = buildEnforcementContractIr();
  built = buildToolTypesBundle(ir);
});
const defsOf = (): Record<string, any> => built.bundle.$defs as Record<string, any>;

it('keeps Unicode schema patterns in generated typed decoders', () => {
  const generated = read('contract/tool_types.rs');
  expect(generated).toContain('::regress::Regex::with_flags(');
  expect(generated).toContain(', "u")');
  expect(generated).toContain('White_Space');
  expect(generated).not.toMatch(/Regex\s*::\s*new\s*\(/);
});

it('omits only tautological property names from the native representation', () => {
  const unrestricted = { type: 'object', propertyNames: { type: 'string' }, additionalProperties: {} };
  const constrained = { type: 'object', propertyNames: { type: 'string', pattern: '^safe_' }, additionalProperties: {} };
  const bundle = { $defs: { Unrestricted: unrestricted, Constrained: constrained } };
  const defs = rustBundle(bundle).$defs as Record<string, any>;
  expect(defs.Unrestricted.propertyNames).toBeUndefined();
  expect(defs.Constrained.propertyNames).toEqual(constrained.propertyNames);
  expect(bundle.$defs.Unrestricted.propertyNames).toEqual({ type: 'string' });
  const namedField = { $defs: { Query: { type: 'object', properties: { propertyNames: { type: 'string' } } } } };
  expect(rustBundle(namedField)).toEqual(namedField);
});

it('normalizes ref-site annotations without dropping intersection constraints', () => {
  const bundle = { $defs: {
    Range: { title: 'Range', description: 'Canonical range', type: 'integer', minimum: 0 },
    Paired: { allOf: [
      { type: 'object', properties: { range: { $ref: '#/$defs/Range', description: 'Observed range', maximum: 10 } } },
      { type: 'object', required: ['range'] },
    ] as Parameters<typeof rustBundle>[0][],
    },
  } };
  const normalized = rustBundle(bundle);
  const defs = normalized.$defs as Record<string, any>;
  expect(defs.Range).toEqual(bundle.$defs.Range);
  expect(defs.Paired.allOf[0].properties.range).toEqual({ $ref: '#/$defs/Range', maximum: 10 });
  expect((bundle.$defs.Paired.allOf[0]!.properties as Record<string, any>).range.description).toBe('Observed range');
  const ajv = new Ajv2020({ strict: false });
  const original = ajv.compile({ ...bundle, $ref: '#/$defs/Paired' });
  const projected = new Ajv2020({ strict: false }).compile({ ...normalized, $ref: '#/$defs/Paired' });
  for (const value of [{ range: 0 }, { range: 10 }, { range: -1 }, { range: 11 }, { range: '0' }, {}]) {
    expect(projected(value)).toBe(original(value));
  }
});

it('retains typed literal fields when the enforcement IR omits implied types', () => {
  const defs = defsOf();
  expect(defs.AstParser.type).toBe('string');
  const filters = defs.GhSearchRepoQuerySearch.properties.filters;
  expect(filters.properties.isSponsorable).toMatchObject({ type: 'boolean', const: true });
  const stopBy = defs.AstSearchRule.properties.stopBy.anyOf;
  expect(stopBy.find((branch: any) => branch.enum)).toMatchObject({
    type: 'string', enum: ['neighbor', 'end'],
  });
});

it('generates the fingerprinted analysis capabilities through the shared bundle', () => {
  const { bundle, fingerprint } = built;
  const defs = defsOf();
  expect(fingerprint).toBe(ir.fingerprint);
  expect(defs.AnalysisCapabilities).toMatchObject({
    type: 'object',
    additionalProperties: false,
    required: ['sourceDecoding', 'grammarCapabilities'],
  });
  expect(defs.AnalysisCapabilities.properties.sourceDecoding).toBeDefined();
  expect(defs.AnalysisCapabilities.properties.grammarCapabilities).toBeDefined();
  expect(defs.AnalysisCapabilities.$ref).toBeUndefined();
  const ajv = new Ajv2020({ strict: false, validateFormats: false });
  const validate = ajv.compile({ ...bundle, $ref: '#/$defs/AnalysisCapabilities' });
  const capability = {
    sourceDecoding: {
      encodings: [{ encoding: 'utf8', bom: 'optional' }],
      malformed: 'rejected', correspondence: 'scalarBoundaries',
    },
    grammarCapabilities: [],
  };
  expect(validate(capability)).toBe(true);
  expect(validate({ ...capability, sourceMappings: true })).toBe(false);
  expect(validate({ ...capability, sourceDecoding: { ...capability.sourceDecoding, malformed: 'replaced' } })).toBe(false);
});

describe('generated tool types', () => {
  it('keeps the per-tool output roots in the Rust bundle, for reachability to prune', () => {
    const bundle = {
      $defs: {
        AQuery: { type: 'object', properties: { r: { $ref: '#/$defs/Shared' } } },
        AInput: { type: 'object', properties: { queries: { items: { $ref: '#/$defs/AQuery' } } } },
        AOutput: { type: 'object', properties: { o: { $ref: '#/$defs/OnlyOut' } } },
        Shared: { type: 'object', properties: { deep: { $ref: '#/$defs/Deep' } } },
        Deep: { type: 'string' },
        OnlyOut: { type: 'string' },
      },
    };
    // Every def stays; `keepReachable` prunes what native never names.
    expect(Object.keys(rustBundle(bundle).$defs as object).sort()).toEqual([
      'AInput',
      'AOutput',
      'AQuery',
      'Deep',
      'OnlyOut',
      'Shared',
    ]);
  });

  it('turns an open union of different output rows into an enum, and keeps overlays merged', () => {
    const defs = rustBundle({
      $defs: {
        AData: { anyOf: [stringRow('issues', 'q'), stringRow('commits', 'q')] },
        BData: { anyOf: [stringRow('q', 'a'), { ...stringRow('q', 'a'), required: ['a'] }] },
        CQuery: { anyOf: [stringRow('issues', 'q'), stringRow('commits', 'q')] },
      },
    }).$defs as Record<string, Record<string, unknown>>;
    expect(defs.AData).toHaveProperty('oneOf');
    expect(defs.BData).toHaveProperty('anyOf');
    // Only output rows (`*Data`) are serialize-only: an input union stays as authored.
    expect(defs.CQuery).toHaveProperty('anyOf');
  });

  it('reuses the canonical query type through every tool input envelope', () => {
    const defs = defsOf();
    for (const tool of ir.tools) {
      const name = String(tool.name);
      const base = name[0]!.toUpperCase() + name.slice(1);
      let rows = 0;
      const visited = new Set<unknown>();
      const visit = (schema: any): void => {
        if (!schema || typeof schema !== 'object' || visited.has(schema)) return;
        visited.add(schema);
        if (schema.$ref) visit(defs[schema.$ref.replace('#/$defs/', '')]);
        if (schema.properties?.queries?.items) {
          rows++;
          expect(schema.properties.queries.items, name).toEqual({
            $ref: `#/$defs/${base}Query`,
          });
        }
        for (const key of ['anyOf', 'oneOf', 'allOf'])
          for (const branch of schema[key] ?? []) visit(branch);
      };
      visit(defs[`${base}Input`]);
      expect(rows, name).toBeGreaterThan(0);
    }
  });

  it('preserves branch acceptance when bundling every canonical input', () => {
    const fixtures = JSON.parse(read('contract/contract-fixtures.json')) as Array<{tool: string; input: unknown}>;
    for (const tool of ir.tools) {
      const name = String(tool.name);
      const base = name.charAt(0).toUpperCase() + name.slice(1);
      const ajv = new Ajv2020({ strict: false, validateFormats: false, allErrors: true });
      const original = ajv.compile({ $defs: ir.$defs, allOf: [tool.inputSchema] } as AnySchema);
      const bundled = ajv.compile({ ...built.bundle, $ref: `#/$defs/${base}Input` });
      for (const fixture of fixtures.filter(row => row.tool === name)) {
        expect(bundled(fixture.input), JSON.stringify(fixture.input)).toBe(original(fixture.input));
      }
    }
  });

  it('keeps overlapping closed unions inclusive in the canonical bundle', () => {
    const schema = {type:'object',additionalProperties:false,properties:{value:{type:'string'}}};
    const bundle = {$defs:{ExampleQuery:{anyOf:[schema,{...schema,required:['value']}]}}};
    const ajv = new Ajv2020({strict:false});
    const original = ajv.compile({...bundle,$ref:'#/$defs/ExampleQuery'});
    expect(original({value:'same'})).toBe(true);
    expect((rustBundle(bundle).$defs as Record<string,unknown>).ExampleQuery).toHaveProperty('oneOf');
    expect(bundle.$defs.ExampleQuery).toHaveProperty('anyOf');
  });

  it('rejects an unnamed recursive schema instead of inventing a name', () => {
    const [first] = ir.tools as unknown as Array<Record<string, unknown>>;
    const unnamed = {
      ...first,
      querySchema: { $defs: { __schema0: { type: 'object' } }, type: 'object' },
    };
    expect(() =>
      buildToolTypesBundle({ ...ir, tools: [unnamed] } as unknown as typeof ir)
    ).toThrow(/unnamed recursive schema/);
  });

  it('rejects a registered schema copied inline beside its $ref, naming the fix', () => {
    const [first] = ir.tools as unknown as Array<Record<string, unknown>>;
    const copied = {
      ...first,
      querySchema: {
        type: 'object',
        properties: {
          terms: { $ref: '#/$defs/TextMatchTerms', title: 'MatchString', type: 'array' },
        },
      },
    };
    expect(() =>
      buildToolTypesBundle({ ...ir, tools: [copied] } as unknown as typeof ir)
    ).toThrow(/properties\/terms: registered schema #\/\$defs\/TextMatchTerms .*describedUse/);
  });
});
