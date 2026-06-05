import { describe, expect, it } from 'vitest';
import { paginationTotal } from '../../src/tools/evidence.js';
import { buildFindFilesEvidence } from '../../src/tools/local_find_files/execution.js';
import { buildViewStructureEvidence } from '../../src/tools/local_view_structure/execution.js';
import { buildRipgrepEvidence } from '../../src/tools/local_ripgrep/execution.js';

describe('paginationTotal', () => {
  it('reads the first present numeric key', () => {
    expect(paginationTotal({ totalFiles: 227 }, 'totalFiles')).toBe(227);
    expect(paginationTotal({ totalEntries: 20 }, 'totalEntries')).toBe(20);
  });
  it('returns 0 for missing, non-record, or non-numeric values', () => {
    expect(paginationTotal(undefined, 'totalFiles')).toBe(0);
    expect(paginationTotal({}, 'totalFiles')).toBe(0);
    expect(paginationTotal({ totalFiles: 'x' }, 'totalFiles')).toBe(0);
  });
});

describe('evidence builders', () => {
  describe('buildFindFilesEvidence', () => {
    it('is answer-ready when pagination reports files even if display array is empty', () => {
      const ev = buildFindFilesEvidence({
        files: [],
        pagination: { hasMore: true, totalFiles: 227 },
      });
      expect(ev.answerReady).toBe(true);
    });
    it('marks incomplete when pagination has more results', () => {
      const ev = buildFindFilesEvidence({
        files: [{ path: 'a.ts' }],
        pagination: { hasMore: true, totalFiles: 227 },
      });
      expect(ev.answerReady).toBe(true);
      expect(ev.complete).toBe(false);
      expect(ev.reason).toContain('File pagination has more results.');
    });
    it('reports not-ready when there are genuinely zero files', () => {
      const ev = buildFindFilesEvidence({
        files: [],
        pagination: { totalFiles: 0 },
      });
      expect(ev.answerReady).toBe(false);
      expect(ev.reason).toContain('No files matched');
    });
  });

  describe('buildViewStructureEvidence', () => {
    it('is answer-ready when pagination reports entries even if display array is empty', () => {
      const ev = buildViewStructureEvidence({
        entries: [],
        pagination: { hasMore: true, totalEntries: 20 },
      });
      expect(ev.answerReady).toBe(true);
    });
    it('still reports an empty view when the tree really is empty', () => {
      const ev = buildViewStructureEvidence({
        entries: [],
        pagination: { totalEntries: 0 },
      });
      expect(ev.answerReady).toBe(false);
      expect(ev.reason).toContain('No directory entries matched');
    });
  });

  describe('buildRipgrepEvidence', () => {
    it('is answer-ready when pagination reports files even if display array is empty', () => {
      const ev = buildRipgrepEvidence({
        files: [],
        pagination: { hasMore: true, totalFiles: 13 },
      });
      expect(ev.answerReady).toBe(true);
    });
    it('marks incomplete when file pagination has more', () => {
      const ev = buildRipgrepEvidence({
        files: [{ path: 'a.ts', matches: [] }],
        pagination: { hasMore: true, totalFiles: 5 },
      });
      expect(ev.complete).toBe(false);
      expect(ev.reason).toContain('File pagination has more results.');
    });
  });
});
