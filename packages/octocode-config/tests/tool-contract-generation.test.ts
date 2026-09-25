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

  it('shares one enum for a vocabulary several tools use', () => {
    const defs = buildToolTypesBundle().bundle.$defs as Record<string, unknown>;
    expect(defs.ChunkType).toEqual({ title: 'ChunkType', type: 'string', enum: ['lines', 'bytes'] });
    const ghChunk = JSON.stringify(defs.GhGetFileContentQuery);
    const localChunk = JSON.stringify(defs.LocalFetchQuery);
    expect(ghChunk).toContain('"$ref":"#/$defs/ChunkType"');
    expect(localChunk).toContain('"$ref":"#/$defs/ChunkType"');
  });

  it('rejects two different schemas under one bundle name', () => {
    const ir = buildEnforcementContractIr();
    const [first] = ir.tools as unknown as Array<Record<string, unknown>>;
    const clash = { ...first, querySchema: { type: 'object', properties: { x: { type: 'string' } } } };
    expect(() =>
      buildToolTypesBundle({ ...ir, tools: [first, clash] } as unknown as typeof ir)
    ).toThrow(/Conflicting tool-type definition/);
  });

  it('types tool rows by name at compile time', () => {
    const query: ToolQuery<'localFetch'> = { path: '/tmp/a.ts', reasoning: 'Read the file' };
    const input: ToolInput<'localFetch'> = { queries: [query] };
    const output = { results: [] } satisfies Partial<ToolOutput<'localFetch'>>;
    expect(input.queries[0]?.path).toBe('/tmp/a.ts');
    expect(output.results).toEqual([]);
  });
});
