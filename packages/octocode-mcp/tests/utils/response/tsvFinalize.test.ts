import { describe, it, expect } from 'vitest';
import {
  finalizeTsv,
  commonDirPrefix,
  relativizeResultPaths,
} from '../../../src/utils/response/tsvFinalize.js';

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

describe('finalizeTsv', () => {
  it('P1: relativizes an absolute path column and emits base', () => {
    const rows = [
      { path: '/root/src/a.ts', n: 1 },
      { path: '/root/src/sub/b.ts', n: 2 },
    ];
    const out = finalizeTsv(['path', 'n'], rows);
    expect(out.base).toBe('/root/src');
    expect(out.rows.map(r => r.path)).toEqual(['a.ts', 'sub/b.ts']);
    expect(out.columns).toContain('path');
  });

  it('P2: drops columns that are empty in every row', () => {
    const rows = [
      { path: '/r/a.ts', accessed: '', created: '' },
      { path: '/r/b.ts', accessed: '', created: '' },
    ];
    const out = finalizeTsv(['path', 'accessed', 'created'], rows);
    expect(out.columns).not.toContain('accessed');
    expect(out.columns).not.toContain('created');
    expect(out.columns).toContain('path');
  });

  it('P3: hoists a column that is identical across all rows into shared', () => {
    const rows = [
      { owner: 'facebook', repo: 'react', path: 'a.ts' },
      { owner: 'facebook', repo: 'react', path: 'b.ts' },
    ];
    const out = finalizeTsv(['owner', 'repo', 'path'], rows);
    expect(out.shared).toEqual({ owner: 'facebook', repo: 'react' });
    expect(out.columns).toEqual(['path']);
  });

  it('P3: does NOT hoist a column that varies', () => {
    const rows = [
      { type: 'file', path: 'a.ts' },
      { type: 'dir', path: 'b' },
    ];
    const out = finalizeTsv(['type', 'path'], rows);
    expect(out.shared).toBeUndefined();
    expect(out.columns).toEqual(['type', 'path']);
  });

  it('never hoists or relativizes a single row', () => {
    const rows = [{ owner: 'x', repo: 'y', path: '/abs/a.ts' }];
    const out = finalizeTsv(['owner', 'repo', 'path'], rows);
    expect(out.base).toBeUndefined();
    expect(out.shared).toBeUndefined();
    expect(out.rows[0].path).toBe('/abs/a.ts');
  });

  it('leaves repo-relative (non-absolute) paths untouched (github tools)', () => {
    const rows = [
      { path: 'packages/a.ts', owner: 'o', repo: 'r' },
      { path: 'packages/b.ts', owner: 'o', repo: 'r' },
    ];
    const out = finalizeTsv(['path', 'owner', 'repo'], rows);
    expect(out.base).toBeUndefined(); // not absolute → no base
    expect(out.rows.map(r => r.path)).toEqual([
      'packages/a.ts',
      'packages/b.ts',
    ]);
    expect(out.shared).toEqual({ owner: 'o', repo: 'r' }); // still hoists constants
  });

  it('relativizes a `uri` column (LSP) when no `path` column is present', () => {
    const rows = [
      { uri: '/w/src/a.ts', line: 10 },
      { uri: '/w/src/lib/b.ts', line: 4 },
    ];
    const out = finalizeTsv(['uri', 'line'], rows);
    expect(out.base).toBe('/w/src');
    expect(out.rows.map(r => r.uri)).toEqual(['a.ts', 'lib/b.ts']);
  });

  it('prefers `path` over `uri` when both columns exist', () => {
    const rows = [
      { path: '/w/p/a.ts', uri: '/other/x.ts' },
      { path: '/w/p/b.ts', uri: '/other/y.ts' },
    ];
    const out = finalizeTsv(['path', 'uri'], rows);
    expect(out.base).toBe('/w/p');
    expect(out.rows.map(r => r.path)).toEqual(['a.ts', 'b.ts']);
    // uri left untouched — only the primary path column is relativized
    expect(out.rows.map(r => r.uri)).toEqual(['/other/x.ts', '/other/y.ts']);
  });

  it('combined: relativize + hoist + drop in one pass', () => {
    const rows = [
      { path: '/w/src/a.ts', type: 'file', accessed: '', perm: '644' },
      { path: '/w/src/b.ts', type: 'file', accessed: '', perm: '644' },
    ];
    const out = finalizeTsv(['path', 'type', 'accessed', 'perm'], rows);
    expect(out.base).toBe('/w/src');
    expect(out.rows.map(r => r.path)).toEqual(['a.ts', 'b.ts']);
    expect(out.shared).toEqual({ type: 'file', perm: '644' });
    expect(out.columns).toEqual(['path']); // type/perm hoisted, accessed dropped
  });
});
