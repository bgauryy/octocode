/**
 * Branch coverage tests for src/providers/pullRequestFileChanges.ts
 *
 * Covers:
 * - countPatchLineChanges: undefined/null patch → early return with zeros
 * - countPatchLineChanges: diff header lines (+++/---) → skipped
 * - countPatchLineChanges: addition/deletion lines
 */

import { describe, it, expect } from 'vitest';
import {
  countPatchLineChanges,
  shapePullRequestFileChanges,
  parseUnifiedDiffByFile,
} from '../../src/providers/pullRequestFileChanges.js';

describe('countPatchLineChanges', () => {
  it('returns zeros when patch is undefined', () => {
    const result = countPatchLineChanges(undefined);
    expect(result).toEqual({ additions: 0, deletions: 0 });
  });

  it('returns zeros when patch is empty string', () => {
    const result = countPatchLineChanges('');
    expect(result).toEqual({ additions: 0, deletions: 0 });
  });

  it('skips +++ and --- header lines', () => {
    const patch = [
      '--- a/src/file.ts',
      '+++ b/src/file.ts',
      '+added line',
      '-removed line',
      ' context line',
    ].join('\n');

    const result = countPatchLineChanges(patch);
    expect(result.additions).toBe(1);
    expect(result.deletions).toBe(1);
  });

  it('counts lines starting with + and -', () => {
    const patch = ['+line1', '+line2', '-line3', ' context'].join('\n');
    const result = countPatchLineChanges(patch);
    expect(result.additions).toBe(2);
    expect(result.deletions).toBe(1);
  });
});

describe('shapePullRequestFileChanges', () => {
  it('returns empty object when fileChanges is empty', () => {
    const result = shapePullRequestFileChanges([], {
      id: 'q1',
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'owner',
      repo: 'repo',
    });
    expect(result).toEqual({});
  });

  it('returns metadata-only view when type is metadata', () => {
    const result = shapePullRequestFileChanges(
      [{ path: 'file.ts', status: 'modified', additions: 5, deletions: 2, patch: 'patch' }],
      {
        id: 'q1',
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'owner',
        repo: 'repo',
        type: 'metadata',
      }
    );
    expect(result.changedFilesCount).toBe(1);
    expect(result.fileChanges?.[0]?.patch).toBeUndefined();
  });

  it('returns full content when type is fullContent', () => {
    const result = shapePullRequestFileChanges(
      [{ path: 'file.ts', status: 'modified', additions: 3, deletions: 1, patch: '+new line' }],
      {
        id: 'q1',
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'owner',
        repo: 'repo',
        type: 'fullContent',
      }
    );
    expect(result.fileChanges?.[0]?.patch).toBe('+new line');
  });
});

describe('parseUnifiedDiffByFile', () => {
  it('returns empty map when diff is undefined', () => {
    const result = parseUnifiedDiffByFile(undefined);
    expect(result.size).toBe(0);
  });

  it('returns empty map when diff is empty string', () => {
    const result = parseUnifiedDiffByFile('');
    expect(result.size).toBe(0);
  });

  it('parses a unified diff into per-file patches', () => {
    const diff = [
      'diff --git a/src/a.ts b/src/a.ts',
      '--- a/src/a.ts',
      '+++ b/src/a.ts',
      '@@ -1,3 +1,4 @@',
      '+new line',
      ' existing',
    ].join('\n');

    const result = parseUnifiedDiffByFile(diff);
    expect(result.size).toBeGreaterThan(0);
    expect(result.has('src/a.ts')).toBe(true);
  });
});
