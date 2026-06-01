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

describe('concise probe evidence (issues #3 / #4)', () => {
  // A concise probe intentionally empties the display array but keeps the count
  // in pagination. The probe's answer IS that count, so answerReady must be true
  // and pagination "has more" (display-only) must not mark it incomplete.
  describe('buildFindFilesEvidence', () => {
    it('is answer-ready and complete for a concise count probe', () => {
      const ev = buildFindFilesEvidence(
        { files: [], pagination: { hasMore: true, totalFiles: 227 } },
        true
      );
      expect(ev.answerReady).toBe(true);
      expect(ev.complete).toBe(true);
      expect(ev.reason).toBeUndefined();
    });
    it('keeps pagination reasons in basic mode', () => {
      const ev = buildFindFilesEvidence(
        {
          files: [{ path: 'a.ts' }],
          pagination: { hasMore: true, totalFiles: 227 },
        },
        false
      );
      expect(ev.answerReady).toBe(true);
      expect(ev.complete).toBe(false);
      expect(ev.reason).toContain('File pagination has more results.');
    });
    it('reports not-ready when there are genuinely zero files', () => {
      const ev = buildFindFilesEvidence(
        { files: [], pagination: { totalFiles: 0 } },
        true
      );
      expect(ev.answerReady).toBe(false);
      expect(ev.reason).toContain('No files matched');
    });
  });

  describe('buildViewStructureEvidence (#4 misleading reason)', () => {
    it('does not claim "no entries matched" when concise dropped a non-empty tree', () => {
      const ev = buildViewStructureEvidence(
        { entries: [], pagination: { hasMore: true, totalEntries: 20 } },
        true
      );
      expect(ev.answerReady).toBe(true);
      expect(ev.complete).toBe(true);
      expect(ev.reason).toBeUndefined();
    });
    it('still reports an empty view when the tree really is empty', () => {
      const ev = buildViewStructureEvidence(
        { entries: [], pagination: { totalEntries: 0 } },
        true
      );
      expect(ev.answerReady).toBe(false);
      expect(ev.reason).toContain('No directory entries matched');
    });
  });

  describe('buildRipgrepEvidence', () => {
    it('is answer-ready and complete for a concise discovery probe', () => {
      const ev = buildRipgrepEvidence(
        { files: [], pagination: { hasMore: true, totalFiles: 13 } },
        true
      );
      expect(ev.answerReady).toBe(true);
      expect(ev.complete).toBe(true);
      expect(ev.reason).toBeUndefined();
    });
  });
});
