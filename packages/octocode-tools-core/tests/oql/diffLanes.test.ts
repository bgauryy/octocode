import { describe, expect, it } from 'vitest';
import {
  classifyDiffLane,
  diffLaneBackend,
} from '../../src/oql/diffLanes.js';

describe('classifyDiffLane — single source of truth for target:"diff"', () => {
  it('classifies a PR-patch lane from {prNumber}', () => {
    const lane = classifyDiffLane({ prNumber: 1 });
    expect(lane).toEqual({ kind: 'prPatch', prNumber: 1 });
    expect(diffLaneBackend(lane)).toBe('ghHistoryResearch');
  });

  it('carries files[] into the PR-patch lane when present', () => {
    const lane = classifyDiffLane({ prNumber: 7, files: ['a.ts', 'b.ts'] });
    expect(lane).toEqual({
      kind: 'prPatch',
      prNumber: 7,
      files: ['a.ts', 'b.ts'],
    });
  });

  it('treats prNumber:0 as a valid PR-patch lane (not falsy fallthrough)', () => {
    const lane = classifyDiffLane({ prNumber: 0 });
    expect(lane.kind).toBe('prPatch');
  });

  it('prNumber:null falls through to the direct-file lane when refs present', () => {
    const lane = classifyDiffLane({
      prNumber: null,
      baseRef: 'main',
      headRef: 'feature',
      path: 'README.md',
    });
    expect(lane).toEqual({
      kind: 'directFile',
      baseRef: 'main',
      headRef: 'feature',
      path: 'README.md',
    });
    expect(diffLaneBackend(lane)).toBe('ghGetFileContent');
  });

  it('classifies a direct-file lane from {baseRef,headRef,path}', () => {
    const lane = classifyDiffLane({
      baseRef: 'v1',
      headRef: 'v2',
      path: 'src/index.ts',
    });
    expect(lane.kind).toBe('directFile');
    expect(diffLaneBackend(lane)).toBe('ghGetFileContent');
  });

  it('returns the neither lane (no backend) for an unrecognized shape', () => {
    for (const params of [undefined, {}, { path: 'x' }, { baseRef: 'a' }]) {
      const lane = classifyDiffLane(params);
      expect(lane).toEqual({ kind: 'neither' });
      expect(diffLaneBackend(lane)).toBe('');
    }
  });
});
