import Ajv2020, { type AnySchema } from 'ajv/dist/2020.js';
import { rustBundle } from '../scripts/tool-contract/rust.ts';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  buildEnforcementContractIr,
  getNativeContractFingerprint,
} from '@octocodeai/octocode-core/schema';
import {
  buildToolTypesBundle,
  generateToolContract,
} from '../scripts/generate-tool-contract.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string): string =>
  readFileSync(resolve(packageRoot, path), 'utf8');

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
  const defs = buildToolTypesBundle(buildEnforcementContractIr()).bundle.$defs as Record<string, any>;
  expect(defs.AstParser.type).toBe('string');
  const filters = defs.GhSearchRepoQuerySearch.properties.filters;
  expect(filters.properties.isSponsorable).toMatchObject({ type: 'boolean', const: true });
  const stopBy = defs.AstSearchRule.properties.stopBy.anyOf;
  expect(stopBy.find((branch: any) => branch.enum)).toMatchObject({
    type: 'string', enum: ['neighbor', 'end'],
  });
}, 30_000);

it('generates the fingerprinted analysis capabilities through the shared bundle', () => {
  const ir = buildEnforcementContractIr();
  const { bundle, fingerprint } = buildToolTypesBundle(ir);
  const defs = bundle.$defs as Record<string, any>;
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
}, 30_000);

// Full-contract generation is an integration check, not a five-second latency assertion.
describe('generated tool types', () => {
  it('are current with the core Zod contract (embed, fixtures, bundle, Rust header)', () => {
    const { files, rustHeader } = generateToolContract();
    for (const [path, content] of files) {
      expect(readFileSync(path, 'utf8'), path).toBe(content);
    }
    expect(read('contract/tool_types.rs').startsWith(rustHeader)).toBe(true);
    expect(rustHeader).toContain(
      `pub const TOOL_TYPES_CONTRACT_FINGERPRINT: &str = "${getNativeContractFingerprint()}";`
    );
  }, 30_000);

  it('pins the Rust types to the exact bundle they were generated from', () => {
    const digest = createHash('sha256')
      .update(read('contract/tool-types.schema.json'))
      .digest('hex');
    expect(read('contract/tool_types.rs')).toContain(
      `pub const TOOL_TYPES_SCHEMA_SHA256: &str = "${digest}";`
    );
  });

  it('drops only the per-tool output roots from the Rust bundle', () => {
    const bundle = {
      $defs: {
        AQuery: {
          type: 'object',
          properties: { r: { $ref: '#/$defs/Shared' } },
        },
        AInput: {
          type: 'object',
          properties: { queries: { items: { $ref: '#/$defs/AQuery' } } },
        },
        AOutput: {
          type: 'object',
          properties: { o: { $ref: '#/$defs/OnlyOut' } },
        },
        Shared: {
          type: 'object',
          properties: { deep: { $ref: '#/$defs/Deep' } },
        },
        Deep: { type: 'string' },
        OnlyOut: { type: 'string' },
      },
    };
    // A core-named definition only an output uses (OnlyOut) stays: native
    // builds output fields from shared types.
    expect(Object.keys(rustBundle(bundle).$defs as object).sort()).toEqual([
      'AInput',
      'AQuery',
      'Deep',
      'OnlyOut',
      'Shared',
    ]);
  });

  it('reuses the canonical query type through every tool input envelope', () => {
    const ir = buildEnforcementContractIr();
    const defs = buildToolTypesBundle(ir).bundle.$defs as Record<string, any>;
    for (const tool of ir.tools) {
      const name = String(tool.name);
      const base = name[0]!.toUpperCase() + name.slice(1);
      let rows = 0;
      const visited = new Set<unknown>();
      const visit = (schema: any): void => {
        if (!schema || typeof schema !== 'object' || visited.has(schema))
          return;
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
  }, 30_000);

  it('preserves branch acceptance when bundling every canonical input', () => {
    const ir = buildEnforcementContractIr();
    const { bundle } = buildToolTypesBundle(ir);
    const fixtures = JSON.parse(read('contract/contract-fixtures.json')) as Array<{tool: string; input: unknown}>;
    for (const tool of ir.tools) {
      const name = String(tool.name);
      const base = name.charAt(0).toUpperCase() + name.slice(1);
      const ajv = new Ajv2020({ strict: false, validateFormats: false, allErrors: true });
      const original = ajv.compile(tool.inputSchema as AnySchema);
      const bundled = ajv.compile({ ...bundle, $ref: `#/$defs/${base}Input` });
      for (const fixture of fixtures.filter(row => row.tool === name)) {
        expect(bundled(fixture.input), JSON.stringify(fixture.input)).toBe(original(fixture.input));
      }
    }
  }, 30_000);

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
    const ir = buildEnforcementContractIr();
    const [first] = ir.tools as unknown as Array<Record<string, unknown>>;
    const unnamed = {
      ...first,
      querySchema: { $defs: { __schema0: { type: 'object' } }, type: 'object' },
    };
    expect(() =>
      buildToolTypesBundle({ ...ir, tools: [unnamed] } as unknown as typeof ir)
    ).toThrow(/unnamed recursive schema/);
  }, 30_000);

});

describe('tool contract generator entry', () => {
  it('runs the generator when called through a symlink', () => {
    // --check is read-only; an empty PATH hides cargo-typify, so a run that reaches main() fails loudly.
    const dir = mkdtempSync(join(tmpdir(), 'contract-link-'));
    try {
      const link = join(dir, 'generate-tool-contract.ts');
      symlinkSync(
        resolve(packageRoot, 'scripts/generate-tool-contract.ts'),
        link
      );
      const run = spawnSync(process.execPath, [link, '--check'], {
        encoding: 'utf8',
        env: { PATH: '' },
        timeout: 25_000,
      });
      expect(run.error).toBeUndefined();
      expect(run.stderr).toMatch(/cargo-typify .*is required/);
      expect(run.status).not.toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 30_000);
});
