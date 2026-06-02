import { describe, it, expect } from 'vitest';
import {
  commonDirPrefix,
  relativizeResultPaths,
  hoistSharedFields,
} from '../../../src/utils/response/pathRelativize.js';

describe('relativizeResultPaths (structuredContent leanness)', () => {
  it('relativizes absolute path fields across results and returns base', () => {
    const results = [
      {
        data: {
          files: [{ path: '/w/src/a.ts', n: 1 }, { path: '/w/src/x/b.ts' }],
        },
      },
      { data: { files: [{ path: '/w/src/c.ts' }] } },
    ];
    const base = relativizeResultPaths(results);
    expect(base).toBe('/w/src');
    expect(results[0]!.data.files.map(f => f.path)).toEqual(['a.ts', 'x/b.ts']);
    expect(results[1]!.data.files[0]!.path).toBe('c.ts');
  });

  it('handles entries[] shape (localViewStructure)', () => {
    const results = [
      { data: { entries: [{ path: '/r/p/a.ts' }, { path: '/r/p/b.ts' }] } },
    ];
    expect(relativizeResultPaths(results)).toBe('/r/p');
    expect(results[0]!.data.entries[0]!.path).toBe('a.ts');
  });

  it('leaves repo-relative paths untouched (no base)', () => {
    const results = [
      { data: { files: [{ path: 'pkg/a.ts' }, { path: 'pkg/b.ts' }] } },
    ];
    expect(relativizeResultPaths(results)).toBeUndefined();
    expect(results[0]!.data.files[0]!.path).toBe('pkg/a.ts');
  });

  it('no-op for a single path', () => {
    const results = [{ data: { files: [{ path: '/abs/only.ts' }] } }];
    expect(relativizeResultPaths(results)).toBeUndefined();
    expect(results[0]!.data.files[0]!.path).toBe('/abs/only.ts');
  });

  it('tolerates null/empty data', () => {
    expect(
      relativizeResultPaths([null, undefined, { data: {} }])
    ).toBeUndefined();
  });

  it('relativizes absolute `uri` fields (LSP locations[]) and returns base', () => {
    const results = [
      {
        data: {
          locations: [
            { uri: '/w/src/a.ts', line: 1 },
            { uri: '/w/src/lib/b.ts', line: 9 },
          ],
        },
      },
      { data: { locations: [{ uri: '/w/src/c.ts', line: 3 }] } },
    ];
    expect(relativizeResultPaths(results)).toBe('/w/src');
    expect(results[0]!.data.locations.map(l => l.uri)).toEqual([
      'a.ts',
      'lib/b.ts',
    ]);
    expect(results[1]!.data.locations[0]!.uri).toBe('c.ts');
  });
});

describe('hoistSharedFields', () => {
  it('hoists scalar fields identical across every leaf and deletes them from leaves', () => {
    const results = [
      {
        data: {
          files: [
            { path: 'a.ts', type: 'f', permissions: '644' },
            { path: 'b.ts', type: 'f', permissions: '644' },
          ],
        },
      },
      { data: { files: [{ path: 'c.ts', type: 'f', permissions: '644' }] } },
    ];
    const shared = hoistSharedFields(results);
    expect(shared).toEqual({ type: 'f', permissions: '644' });
    expect(results[0]!.data.files[0]).toEqual({ path: 'a.ts' });
    expect(results[1]!.data.files[0]).toEqual({ path: 'c.ts' });
  });

  it('does NOT hoist a field that varies', () => {
    const results = [
      {
        data: {
          entries: [
            { name: 'a', type: 'f' },
            { name: 'b', type: 'd' },
          ],
        },
      },
    ];
    // single result but 2 leaves → eligible; type varies so nothing hoists
    expect(hoistSharedFields(results)).toBeUndefined();
  });

  it('never hoists path/uri (owned by base relativization)', () => {
    const results = [
      {
        data: {
          locations: [
            { uri: 'x.ts', line: 1 },
            { uri: 'x.ts', line: 2 },
          ],
        },
      },
    ];
    // uri is identical but must stay; line varies → nothing else to hoist
    expect(hoistSharedFields(results)).toBeUndefined();
    expect(results[0]!.data.locations[0]!.uri).toBe('x.ts');
  });

  it('never hoists chaining-identity keys (owner/repo/name/id)', () => {
    const results = [
      {
        data: {
          repositories: [
            { owner: 'org', repo: 'a', name: 'a', id: 'x', language: 'TS' },
            { owner: 'org', repo: 'b', name: 'a', id: 'x', language: 'TS' },
          ],
        },
      },
    ];
    // language is the only non-identity constant → only it hoists.
    expect(hoistSharedFields(results)).toEqual({ language: 'TS' });
    expect(results[0]!.data.repositories[0]).toMatchObject({
      owner: 'org',
      repo: 'a',
      name: 'a',
      id: 'x',
    });
    expect(results[0]!.data.repositories[0]).not.toHaveProperty('language');
  });

  it('no-op for fewer than two leaves', () => {
    expect(
      hoistSharedFields([{ data: { files: [{ path: 'a.ts', type: 'f' }] } }])
    ).toBeUndefined();
  });

  it('preserves numbers and booleans as typed values', () => {
    const results = [
      { data: { items: [{ id: 1, ok: true, mode: 100 }] } },
      { data: { items: [{ id: 2, ok: true, mode: 100 }] } },
    ];
    expect(hoistSharedFields(results)).toEqual({ ok: true, mode: 100 });
  });
});

describe('commonDirPrefix', () => {
  it('returns the deepest shared directory (no trailing slash)', () => {
    expect(commonDirPrefix(['/a/b/c/x.ts', '/a/b/c/y.ts', '/a/b/d/z.ts'])).toBe(
      '/a/b'
    );
  });
  it('returns "" when there is no shared directory', () => {
    expect(commonDirPrefix(['/x/a.ts', '/y/b.ts'])).toBe('');
  });
  it('does not split mid-segment', () => {
    // shared char-prefix is "/a/foo" but the real dir boundary is "/a"
    expect(commonDirPrefix(['/a/foobar.ts', '/a/foobaz.ts'])).toBe('/a');
  });
});
