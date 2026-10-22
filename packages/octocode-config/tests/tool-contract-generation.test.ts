import Ajv2020 from 'ajv/dist/2020.js';
import { rustBundle } from '../scripts/tool-contract/rust.ts';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  TOOL_NAMES,
  buildEnforcementContractIr,
  getNativeContractFingerprint,
} from '@octocodeai/octocode-core/schema';
import type {
  ToolInput,
  ToolOutput,
  ToolQuery,
} from '../src/contracts/schema.js';
import { TOOL_TYPES_CONTRACT_FINGERPRINT } from '../src/contracts/schema.js';
import {
  buildToolTypesBundle,
  generateToolContract,
} from '../scripts/generate-tool-contract.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string): string =>
  readFileSync(resolve(packageRoot, path), 'utf8');

describe('generated tool types', () => {
  it('are current with the core Zod contract (embed, fixtures, TS, bundle, Rust header)', async () => {
    const { files, rustHeader } = await generateToolContract();
    for (const [path, content] of files) {
      expect(readFileSync(path, 'utf8'), path).toBe(content);
    }
    expect(read('contract/tool_types.rs').startsWith(rustHeader)).toBe(true);
    expect(TOOL_TYPES_CONTRACT_FINGERPRINT).toBe(
      getNativeContractFingerprint()
    );
  });

  it('pins the Rust types to the exact bundle they were generated from', () => {
    const digest = createHash('sha256')
      .update(read('contract/tool-types.schema.json'))
      .digest('hex');
    expect(read('contract/tool_types.rs')).toContain(
      `pub const TOOL_TYPES_SCHEMA_SHA256: &str = "${digest}";`
    );
  });

  it('declares query, input, and output types in TypeScript; Rust gets only what native parses', () => {
    const ts = read('src/contracts/toolTypes.generated.ts');
    const rust = read('contract/tool_types.rs');
    for (const tool of Object.values(TOOL_NAMES)) {
      const base = `${tool.charAt(0).toUpperCase()}${tool.slice(1)}`;
      for (const suffix of ['Query', 'Input', 'Output']) {
        expect(ts, `${base}${suffix} (TS)`).toMatch(
          new RegExp(`^export (?:interface|type) ${base}${suffix}\\b`, 'm')
        );
      }
      // Native checks inputs and outputs against the JSON schema and
      // decodes rows: envelope and output types would be dead code (the
      // rest is trimmed to what native names; see rust-reach.test.ts).
      for (const suffix of ['Input', 'Output']) {
        expect(rust, `${base}${suffix} (Rust)`).not.toMatch(
          new RegExp(`^pub (?:struct|enum) ${base}${suffix}\\b`, 'm')
        );
      }
    }
    expect(rust).toMatch(/^pub struct LocalFetchQuery\b/m);
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

  it('names shared types only where core names them (titles and ids)', () => {
    const defs = buildToolTypesBundle().bundle.$defs as Record<string, unknown>;
    expect(defs.WindowUnit).toEqual({
      title: 'WindowUnit',
      type: 'string',
      enum: ['lines', 'bytes'],
    });
    for (const tool of ['GhGetFileContentQuery', 'LocalFetchQuery']) {
      expect(JSON.stringify(defs[tool])).toContain(
        '"$ref":"#/$defs/WindowUnit"'
      );
    }
    expect(defs.AstSearchRule).toBeDefined();
    // No positional or heuristic names leak out of the bundle.
    expect(
      Object.keys(defs).filter(name => /^Shared|__schema/.test(name))
    ).toEqual([]);
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
  });

  it('keeps union semantics in the published bundle and enforces string consts', () => {
    const { bundle } = buildToolTypesBundle();
    const defs = bundle.$defs as Record<string, Record<string, unknown>>;
    const refs = [
      '#/$defs/AstSearchQueryMatchPattern',
      '#/$defs/AstSearchQueryMatchRule',
      '#/$defs/AstSearchQuerySyntaxTree',
      '#/$defs/AstSearchQuerySymbols',
    ];
    // Closed objects can overlap, so the canonical union stays `anyOf`
    // (at least one branch); only the Rust representation becomes `oneOf`.
    const ast = defs.AstSearchQuery as { anyOf: Array<{ $ref: string }> };
    expect(ast.anyOf.map(branch => branch.$ref)).toEqual(refs);
    const rust = (
      rustBundle(bundle).$defs as Record<
        string,
        { oneOf?: Array<{ $ref: string }> }
      >
    ).AstSearchQuery;
    expect(rust?.oneOf?.map(branch => branch.$ref)).toEqual(refs);
    const symbols = defs.AstSearchQuerySymbols as {
      properties: Record<string, unknown>;
    };
    expect(symbols.properties.operation).toEqual({
      type: 'string',
      enum: ['symbols'],
    });
    // typify drops a string const (emits a free String); a one-value enum is
    // enforced. Boolean consts stay a contract-validator check.
    expect(JSON.stringify(defs)).not.toMatch(/"const":"/);
  });

  it('accepts a query that matches more than one closed branch, as the canonical schema does', () => {
    const { bundle } = buildToolTypesBundle();
    const ajv = new Ajv2020({ strict: false, allErrors: true });
    const validate = ajv.compile({ ...bundle, $ref: '#/$defs/LspSearchQuery' });
    const overlapping = {
      operation: 'workspaceSymbol',
      symbolName: 'thing',
      path: '/tmp/project/a.ts',
      workspaceRoot: '/tmp/project',
    };
    expect(validate(overlapping), JSON.stringify(validate.errors)).toBe(true);
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
  });

  it('types tool rows by name at compile time', () => {
    // Briefs are optional: a bare row and a briefed row both type-check.
    const query: ToolQuery<'localFetch'> = { path: '/tmp/a.ts' };
    const briefed: ToolQuery<'localFetch'> = {
      path: '/tmp/a.ts',
      mainGoal: 'Read the file.',
      reasoning: 'Read the file',
    };
    const input = {
      queries: [query, briefed],
    } satisfies ToolInput<'localFetch'>;
    const saved = {
      responseSnapshot: 'saved',
      responseOffset: 1,
    } satisfies ToolInput<'localFetch'>;
    const output = { results: [] } satisfies Partial<ToolOutput<'localFetch'>>;
    expect(input.queries[0]?.path).toBe('/tmp/a.ts');
    expect(saved.responseSnapshot).toBe('saved');
    expect(saved).not.toHaveProperty('queries');
    expect(output.results).toEqual([]);
  });

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
      });
      expect(run.stderr).toMatch(/cargo-typify .*is required/);
      expect(run.status).not.toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
