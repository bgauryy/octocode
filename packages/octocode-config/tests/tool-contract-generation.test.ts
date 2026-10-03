import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  TOOL_NAMES,
  buildEnforcementContractIr,
  getNativeContractFingerprint,
} from '@octocodeai/octocode-core/schema';
import type { ToolInput, ToolOutput, ToolQuery } from '../src/contracts/schema.js';
import { TOOL_TYPES_CONTRACT_FINGERPRINT } from '../src/contracts/schema.js';
import {
  buildToolTypesBundle,
  generateToolContract,
} from '../scripts/generate-tool-contract.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string): string => readFileSync(resolve(packageRoot, path), 'utf8');

describe('generated tool types', () => {
  it('are current with the core Zod contract (embed, fixtures, TS, bundle, Rust header)', async () => {
    const { files, rustHeader } = await generateToolContract();
    for (const [path, content] of files) {
      expect(readFileSync(path, 'utf8'), path).toBe(content);
    }
    expect(read('contract/tool_types.rs').startsWith(rustHeader)).toBe(true);
    expect(TOOL_TYPES_CONTRACT_FINGERPRINT).toBe(getNativeContractFingerprint());
  });

  it('pins the Rust types to the exact bundle they were generated from', () => {
    const digest = createHash('sha256').update(read('contract/tool-types.schema.json')).digest('hex');
    expect(read('contract/tool_types.rs')).toContain(
      `pub const TOOL_TYPES_SCHEMA_SHA256: &str = "${digest}";`
    );
  });

  it('declares a query, input, and output type for every catalog tool in both languages', () => {
    const ts = read('src/contracts/toolTypes.generated.ts');
    const rust = read('contract/tool_types.rs');
    for (const tool of Object.values(TOOL_NAMES)) {
      const base = `${tool.charAt(0).toUpperCase()}${tool.slice(1)}`;
      for (const suffix of ['Query', 'Input', 'Output']) {
        expect(ts, `${base}${suffix} (TS)`).toMatch(
          new RegExp(`^export (?:interface|type) ${base}${suffix}\\b`, 'm')
        );
        expect(rust, `${base}${suffix} (Rust)`).toMatch(
          new RegExp(`^pub (?:struct|enum) ${base}${suffix}\\b`, 'm')
        );
      }
    }
  });

  it('names shared types only where core names them (titles and ids)', () => {
    const defs = buildToolTypesBundle().bundle.$defs as Record<string, unknown>;
    expect(defs.ChunkType).toEqual({ title: 'ChunkType', type: 'string', enum: ['lines', 'bytes'] });
    for (const tool of ['GhGetFileContentQuery', 'LocalFetchQuery']) {
      expect(JSON.stringify(defs[tool])).toContain('"$ref":"#/$defs/ChunkType"');
    }
    expect(defs.AstRule).toBeDefined();
    // No positional or heuristic names leak out of the bundle.
    expect(Object.keys(defs).filter((name) => /^Shared|__schema/.test(name))).toEqual([]);
  });

  it('turns closed anyOf unions into oneOf and enforces string consts', () => {
    const defs = buildToolTypesBundle().bundle.$defs as Record<string, Record<string, unknown>>;
    const ast = defs.AstSearchQuery as { oneOf: Array<{ $ref: string }> };
    expect(ast.oneOf.map((branch) => branch.$ref)).toEqual([
      '#/$defs/AstSearchQueryMatchPattern',
      '#/$defs/AstSearchQueryMatchRule',
      '#/$defs/AstSearchQuerySyntaxTree',
      '#/$defs/AstSearchQuerySymbols',
    ]);
    const symbols = defs.AstSearchQuerySymbols as { properties: Record<string, unknown> };
    expect(symbols.properties.operation).toEqual({ type: 'string', enum: ['symbols'] });
    // typify drops a string const (emits a free String); a one-value enum is
    // enforced. Boolean consts stay a contract-validator check.
    expect(JSON.stringify(defs)).not.toMatch(/"const":"/);
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
    const briefed: ToolQuery<'localFetch'> = { path: '/tmp/a.ts', mainGoal: 'Read the file.', reasoning: 'Read the file' };
    const input: ToolInput<'localFetch'> = { queries: [query, briefed] };
    const output = { results: [] } satisfies Partial<ToolOutput<'localFetch'>>;
    expect(input.queries[0]?.path).toBe('/tmp/a.ts');
    expect(output.results).toEqual([]);
  });
});
