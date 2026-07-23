import { describe, expect, it } from 'vitest';
import { hoistSharedFields } from '../../../src/utils/response/pathRelativize.js';

describe('hoistSharedFields', () => {
  it('hoists constant scalar fields shared across leaves and strips them from entries', () => {
    const leaves = [
      { lang: 'ts', size: '1KB' },
      { lang: 'ts', size: '2KB' },
    ];
    const results = [{ data: { files: leaves } }];

    const shared = hoistSharedFields(results);

    expect(shared).toEqual({ lang: 'ts' });
    expect(leaves.every(l => !('lang' in l))).toBe(true);
    expect(leaves.map(l => l.size)).toEqual(['1KB', '2KB']);
  });

  it('never hoists `type` — required by output schemas (regression: localViewStructure entries[].type)', () => {
    // A homogeneous listing (filesOnly / directoriesOnly / all-files dir with
    // details:true) makes `type` constant across every entry. Hoisting it out
    // violated ViewStructureEntrySchema (type is required per entry) and the
    // MCP SDK rejected the whole batch with -32602.
    const entries = [
      { type: 'file', size: '399.0B', permissions: 'rw-r--r--' },
      { type: 'file', size: '1.4KB', permissions: 'rw-r--r--' },
      { type: 'file', size: '2.9KB', permissions: 'rw-r--r--' },
    ];
    const results = [{ data: { entries } }];

    const shared = hoistSharedFields(results);

    expect(shared?.type).toBeUndefined();
    expect(entries.every(e => e.type === 'file')).toBe(true);
    // Other constant scalars (permissions) may still be hoisted.
    expect(shared?.permissions).toBe('rw-r--r--');
    expect(entries.every(e => !('permissions' in e))).toBe(true);
  });

  it('returns undefined for fewer than two leaves', () => {
    expect(
      hoistSharedFields([{ data: { files: [{ type: 'file', a: 1 }] } }])
    ).toBeUndefined();
  });
});
