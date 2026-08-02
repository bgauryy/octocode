import { describe, expect, it } from 'vitest';

import {
  DirectToolInputError,
  prepareDirectToolInput,
} from '../../src/tools/directToolCatalog.meta.js';

type Prepared = { queries: Array<Record<string, unknown>> };

const prep = (tool: string, query: Record<string, unknown>): Prepared =>
  prepareDirectToolInput(tool, query, {
    rejectUnknownFields: true,
  }) as Prepared;

/**
 * Regression guards for the five first-contact field misses measured in
 * benchmark run compare-run-20260802-b (octocode-vs-gh / octocode-vs-gh-rtk,
 * Arm B call logs): each one cost a full agent turn on a retry. All are
 * plausible renames of a real field, so they are folded — not rejected.
 */
describe('benchmark-measured field aliases (compare-run-20260802-b)', () => {
  it('localSearchCode: language → langType', () => {
    const q = prep('localSearchCode', {
      path: '/tmp',
      pattern: 'ref($$$ARGS)',
      mode: 'structural',
      language: 'typescript',
    }).queries[0]!;
    expect(q.langType).toBe('typescript');
    expect(q.language).toBeUndefined();
  });

  it('ghGetFileContent: matchStringContextLines → contextLines', () => {
    const q = prep('ghGetFileContent', {
      owner: 'o',
      repo: 'r',
      path: 'f.ts',
      matchString: 'x',
      matchStringContextLines: 3,
    }).queries[0]!;
    expect(q.contextLines).toBe(3);
    expect(q.matchStringContextLines).toBeUndefined();
  });

  it('ghGetFileContent: minified:true → minify:"standard", minified:false → "none"', () => {
    const on = prep('ghGetFileContent', {
      owner: 'o',
      repo: 'r',
      path: 'f.ts',
      minified: true,
    }).queries[0]!;
    expect(on.minify).toBe('standard');
    expect(on.minified).toBeUndefined();

    const off = prep('ghGetFileContent', {
      owner: 'o',
      repo: 'r',
      path: 'f.ts',
      minified: false,
    }).queries[0]!;
    expect(off.minify).toBe('none');
  });

  it('ghGetFileContent: minified with a valid enum string folds through to minify', () => {
    const q = prep('ghGetFileContent', {
      owner: 'o',
      repo: 'r',
      path: 'f.ts',
      minified: 'symbols',
    }).queries[0]!;
    expect(q.minify).toBe('symbols');
  });

  it('ghSearchPullRequests: merged:true → state:"merged"', () => {
    const q = prep('ghSearchPullRequests', {
      owner: 'o',
      repo: 'r',
      merged: true,
    }).queries[0]!;
    expect(q.state).toBe('merged');
    expect(q.merged).toBeUndefined();
  });

  it('ghSearchPullRequests: merged:false is NOT foldable (still rejected)', () => {
    // "closed but not merged" is not expressible via state — folding false
    // would silently change meaning, so it must keep erroring.
    expect(() =>
      prep('ghSearchPullRequests', { owner: 'o', repo: 'r', merged: false })
    ).toThrow(DirectToolInputError);
  });

  it('ghSearchCommits: filePath → path', () => {
    const q = prep('ghSearchCommits', {
      owner: 'o',
      repo: 'r',
      filePath: 'src/a.ts',
    }).queries[0]!;
    expect(q.path).toBe('src/a.ts');
    expect(q.filePath).toBeUndefined();
  });

  it('localFindFiles: maxResults → limit', () => {
    const q = prep('localFindFiles', {
      path: '/tmp',
      maxResults: 5,
    }).queries[0]!;
    expect(q.limit).toBe(5);
    expect(q.maxResults).toBeUndefined();
  });

  it('an alias never clobbers an explicitly-set canonical field', () => {
    const q = prep('ghGetFileContent', {
      owner: 'o',
      repo: 'r',
      path: 'f.ts',
      minify: 'none',
      minified: true,
    }).queries[0]!;
    expect(q.minify).toBe('none');
  });
});
