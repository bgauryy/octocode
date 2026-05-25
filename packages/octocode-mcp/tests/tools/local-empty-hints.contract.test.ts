/**
 * Per-local-tool empty-result hint contract.
 *
 * Mirrors the remote-tool contract: when a local tool returns zero
 * results, the response must carry a query-aware recovery line that
 * names the actual filters in play.
 */

import { describe, it, expect } from 'vitest';
import { hints as ripgrepHints } from '../../src/tools/local_ripgrep/hints.js';
import { hints as findFilesHints } from '../../src/tools/local_find_files/hints.js';
import { hints as viewStructureHints } from '../../src/tools/local_view_structure/hints.js';

function flatten(arr: ReturnType<typeof ripgrepHints.empty>): string[] {
  return arr.filter((s): s is string => typeof s === 'string');
}

describe('localSearchCode.empty — query-aware recovery', () => {
  it('names the filters in play (type + excludeDir) in the recovery line', () => {
    const h = flatten(
      ripgrepHints.empty!({
        pattern: 'handler',
        path: 'src',
        type: 'ts',
        excludeDir: ['node_modules', 'dist'],
      } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('type="ts"'))).toBe(true);
    expect(h.some(s => s.includes('excludeDir='))).toBe(true);
    expect(h.some(s => s.includes('src'))).toBe(true);
  });

  it('warns about short patterns when fixedString is not set', () => {
    const h = flatten(
      ripgrepHints.empty!({
        pattern: 'xy',
        path: 'src',
      } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('"xy" is short'))).toBe(true);
  });

  it('flags caseSensitive=true when the search came back empty', () => {
    const h = flatten(
      ripgrepHints.empty!({
        pattern: 'Handler',
        path: 'src',
        caseSensitive: true,
      } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('caseSensitive=true'))).toBe(true);
  });

  it('falls back to a generic-but-actionable line when no filters are set', () => {
    const h = flatten(ripgrepHints.empty!({}));
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toMatch(
      /Broaden the pattern|type\/include filters|different path/
    );
  });
});

describe('localFindFiles.empty — query-aware recovery', () => {
  it('names the filter that produced zero matches', () => {
    const h = flatten(
      findFilesHints.empty!({
        path: 'src',
        name: '*.test.ts',
      } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('name="*.test.ts"'))).toBe(true);
    expect(h.some(s => s.includes('src'))).toBe(true);
  });

  it('flags multiple-filter overload', () => {
    const h = flatten(
      findFilesHints.empty!({
        path: 'src',
        name: '*.ts',
        modifiedWithin: '1d',
        sizeGreater: '1M',
      } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('Drop one filter'))).toBe(true);
    expect(h.some(s => s.includes('modifiedWithin'))).toBe(true);
    expect(h.some(s => s.includes('sizeGreater'))).toBe(true);
  });

  it('falls back to a path-verify line when no filters are set', () => {
    const h = flatten(
      findFilesHints.empty!({ path: '/missing' } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('Verify the path'))).toBe(true);
    expect(h.some(s => s.includes('/missing'))).toBe(true);
  });
});

describe('localViewStructure.empty — query-aware recovery', () => {
  it('names extension/pattern filters when present', () => {
    const h = flatten(
      viewStructureHints.empty!({
        path: 'src',
        extension: '.ts',
        pattern: 'index',
      } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('extension=".ts"'))).toBe(true);
    expect(h.some(s => s.includes('pattern="index"'))).toBe(true);
    expect(h.some(s => s.includes('src'))).toBe(true);
  });

  it('emits a parent-directory recovery line for empty dirs without filters', () => {
    const h = flatten(
      viewStructureHints.empty!({ path: '/repo/empty' } as Record<
        string,
        unknown
      >)
    );
    expect(h.some(s => s.includes('Empty directory'))).toBe(true);
    expect(h.some(s => s.includes('parent directory'))).toBe(true);
  });
});
