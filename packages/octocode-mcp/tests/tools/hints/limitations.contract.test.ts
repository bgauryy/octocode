import { describe, it, expect } from 'vitest';

import { hints as ripgrepHints } from '../../../../octocode-tools-core/src/tools/local_ripgrep/hints.js';
import { hints as fetchContentHints } from '../../../../octocode-tools-core/src/tools/local_fetch_content/hints.js';
import { hints as viewStructureHints } from '../../../../octocode-tools-core/src/tools/local_view_structure/hints.js';
import { hints as ghFetchHints } from '../../../../octocode-tools-core/src/tools/github_fetch_content/hints.js';
import { hints as cloneHints } from '../../../../octocode-tools-core/src/tools/github_clone_repo/hints.js';

const BANNED_WORKFLOW_PHRASES = [
  'Best approach',
  'Use matchString',
  'Use charLength',
  'Use charOffset',
  'Why matchString',
  'Use localSearchCode',
  'Use localFindFiles',
  'Use localGetFileContent',
  'Each location =',
  'Each incomingCall',
  'Each outgoingCall',
  'Found ',
  'Tip:',
  'Browse:',
  'Install:',
  'Critical:',
  'Alternative:',
  'PIVOT',
];

function assertLean(hint: string) {
  for (const phrase of BANNED_WORKFLOW_PHRASES) {
    expect(
      hint,
      `"${hint}" contains banned workflow phrase "${phrase}"`
    ).not.toContain(phrase);
  }
  expect(hint.split('\n').length).toBeLessThanOrEqual(2);
  expect(hint.length).toBeLessThanOrEqual(140);
}

describe('limitation hints — content shape', () => {
  it('localSearchCode size_limit with matchCount', () => {
    const h = ripgrepHints.error({
      errorType: 'size_limit',
      matchCount: 1500,
    } as never);
    expect(h).toHaveLength(1);
    expect(h[0]).toContain('1500');
    assertLean(h[0]!);
  });

  it('localGetFileContent size_limit + isLarge', () => {
    const h = fetchContentHints.error({
      errorType: 'size_limit',
      isLarge: true,
      fileSize: 800_000,
    } as never);
    expect(h.length).toBeGreaterThanOrEqual(1);
    expect(h[0]).toMatch(/~\d+KB/);
    assertLean(h[0]!);
  });

  it('localViewStructure size_limit returns [] (cap is surfaced as runtime warning via wasCapped)', () => {
    const h = viewStructureHints.error({
      errorType: 'size_limit',
      entryCount: 5000,
    } as never);
    expect(h).toHaveLength(0);
  });

  it('ghGetFileContent 300KB cap', () => {
    const h = ghFetchHints.error({
      errorType: 'size_limit',
      fileSize: 350,
    } as never);
    expect(h.length).toBeGreaterThanOrEqual(1);
    expect(h[0]).toContain('350KB');
    assertLean(h[0]!);
  });

  it('ghCloneRepo error one-liners', () => {
    for (const errorType of ['permission', 'not_found', 'timeout']) {
      const h = cloneHints.error({ errorType } as never);
      expect(h).toHaveLength(1);
      assertLean(h[0]!);
    }
  });
});

describe('limitation hints — silence when threshold not hit', () => {
  it('localSearchCode without size_limit errorType returns []', () => {
    expect(ripgrepHints.error({ errorType: 'other' as never })).toEqual([]);
  });

  it('localGetFileContent size_limit always fires (isLarge no longer required)', () => {
    const h = fetchContentHints.error({
      errorType: 'size_limit',
      fileSize: 100,
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toMatch(/too large|matchString/);
  });

  it('localViewStructure without entryCount stays silent', () => {
    expect(
      viewStructureHints.error({ errorType: 'size_limit' } as never)
    ).toEqual([]);
  });
});
