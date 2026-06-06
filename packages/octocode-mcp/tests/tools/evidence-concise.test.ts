import { describe, expect, it } from 'vitest';
import {
  paginationTotal,
  buildEvidenceMetadata,
} from '../../src/tools/evidence.js';
import { buildFindFilesEvidence } from '../../src/tools/local_find_files/execution.js';
import { buildViewStructureEvidence } from '../../src/tools/local_view_structure/execution.js';
import { buildRipgrepEvidence } from '../../src/tools/local_ripgrep/execution.js';

describe('buildEvidenceMetadata — complete=false always has reason', () => {
  it('evidence complete=false result always has at least one reason string', () => {
    const result = buildEvidenceMetadata({
      kind: 'code',
      answerReady: true,
      emptyReason: 'No results found.',
      incompleteReasons: ['partial result'],
    });
    expect(result.complete).toBe(false);
    expect(result.reason?.length).toBeGreaterThan(0);
  });

  it('evidence complete=false with multiple reasons joins them', () => {
    const result = buildEvidenceMetadata({
      kind: 'code',
      answerReady: true,
      emptyReason: 'No results found.',
      incompleteReasons: ['first', 'second', 'third'],
    });
    expect(result.complete).toBe(false);
    expect(result.reason).toContain('first');
    expect(result.reason).toContain('second');
    expect(result.reason).toContain('third');
  });

  it('evidence complete=true (no incompleteReasons) has no reason when answerReady', () => {
    const result = buildEvidenceMetadata({
      kind: 'code',
      answerReady: true,
      emptyReason: 'No results found.',
    });
    expect(result.complete).toBe(true);
    expect(result.reason).toBeUndefined();
  });

  it('evidence complete=true but not answer-ready uses emptyReason', () => {
    const result = buildEvidenceMetadata({
      kind: 'code',
      answerReady: false,
      emptyReason: 'No results found.',
    });
    expect(result.complete).toBe(true);
    expect(result.reason).toBe('No results found.');
  });
});

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
