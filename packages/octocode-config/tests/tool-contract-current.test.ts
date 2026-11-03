import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, expect, it } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/octocode-core/schema';
import { generateToolContract } from '../scripts/generate-tool-contract.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string): string =>
  readFileSync(resolve(packageRoot, path), 'utf8');

// Full-contract generation is an integration check; its own file lets vitest run it beside the
// other contract checks instead of after them.
let generated: ReturnType<typeof generateToolContract>;
beforeAll(() => {
  generated = generateToolContract();
});

it('generated tool types are current with the core Zod contract (embed, fixtures, bundle, Rust header)', () => {
  const { files, rustHeader } = generated;
  for (const [path, content] of files) {
    expect(readFileSync(path, 'utf8'), path).toBe(content);
  }
  expect(read('contract/tool_types.rs').startsWith(rustHeader)).toBe(true);
  expect(rustHeader).toContain(
    `pub const TOOL_TYPES_CONTRACT_FINGERPRINT: &str = "${getNativeContractFingerprint()}";`
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

// A row `next` key pattern as core writes it: channel names, each with an optional
// branch (2, 3, …) or numbered (0, 1, …) suffix.
const NEXT_KEY_PATTERN = /^\^\(\?:(?:[A-Za-z]+(?:\(\?:\[2-9\]\|\[1-9\]\[0-9\]\+\)\?|\(\?:0\|\[1-9\]\[0-9\]\*\))?\|?)+\)\$$/;

it('declares every next key an output schema admits, error-row channels included', () => {
  const contract = JSON.parse(read('contract/tool-contract.json'));
  const declared = new Set<string>([
    ...Object.values<{ row: object }>(contract.continuationChannels).flatMap(({ row }) => Object.keys(row)),
    ...Object.keys(contract.errorContinuationChannels ?? {}),
  ]);
  const admitted = new Set<string>();
  JSON.stringify(contract, (key, value) => {
    if (key === 'pattern' && typeof value === 'string' && NEXT_KEY_PATTERN.test(value))
      for (const name of value.slice(4, -2).match(/\b[A-Za-z]+\b/g) ?? []) admitted.add(name);
    return value;
  });
  expect(admitted.size).toBeGreaterThan(0);
  expect([...admitted].filter((name) => !declared.has(name))).toEqual([]);
});
