/**
 * Limitation / cap hint contract.
 *
 * Every tool with a hard size/result cap emits an evidence-conditional
 * one-liner naming the offending value. Workflow followup text is OUT —
 * tool descriptions own that.
 *
 * Coverage:
 *   - localSearchCode      → size_limit + matchCount
 *   - localGetFileContent  → fileTooLarge with fileSize in KB
 *   - localViewStructure   → size_limit + entryCount
 *   - localFindFiles       → "Results capped at N of M" (executor-side)
 *   - githubGetFileContent → 300KB cap with fileSize
 *   - githubSearchCode     → match-value truncation warning (structured)
 *   - githubCloneRepo      → cache marker only
 *   - lspCallHierarchy     → depth/timeout warning
 *
 * Banned phrases — every limitation line is checked against this list to
 * ensure no workflow novella sneaks back in.
 */

import { describe, it, expect } from 'vitest';

import { hints as ripgrepHints } from '../../../src/tools/local_ripgrep/hints.js';
import { hints as fetchContentHints } from '../../../src/tools/local_fetch_content/hints.js';
import { hints as viewStructureHints } from '../../../src/tools/local_view_structure/hints.js';
import { hints as ghFetchHints } from '../../../src/tools/github_fetch_content/hints.js';
import { hints as cloneHints } from '../../../src/tools/github_clone_repo/hints.js';
import { hints as callHints } from '../../../src/tools/lsp_call_hierarchy/hints.js';

const BANNED_WORKFLOW_PHRASES = [
  'Best approach',
  'Use matchString',
  'Use charLength',
  'Use charOffset',
  'Why matchString',
  'Use lspFindReferences',
  'Use lspGotoDefinition',
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
  // One-line guarantee
  expect(hint.split('\n').length).toBeLessThanOrEqual(2);
  // Length budget — limitation hints should be < 120 chars (single evidence line).
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
    expect(h).toHaveLength(1);
    expect(h[0]).toMatch(/~\d+KB/);
    assertLean(h[0]!);
  });

  it('localViewStructure size_limit + entryCount', () => {
    const h = viewStructureHints.error({
      errorType: 'size_limit',
      entryCount: 5000,
    } as never);
    expect(h).toHaveLength(1);
    expect(h[0]).toContain('5000');
    assertLean(h[0]!);
  });

  it('githubGetFileContent 300KB cap', () => {
    const h = ghFetchHints.error({
      errorType: 'size_limit',
      fileSize: 350,
    } as never);
    expect(h).toHaveLength(1);
    expect(h[0]).toContain('350KB');
    expect(h[0]).toContain('300KB');
    assertLean(h[0]!);
  });

  it('githubCloneRepo error one-liners', () => {
    for (const errorType of ['permission', 'not_found', 'timeout']) {
      const h = cloneHints.error({ errorType } as never);
      expect(h).toHaveLength(1);
      assertLean(h[0]!);
    }
  });

  it('lspCallHierarchy timeout cites depth', () => {
    const h = callHints.error({
      errorType: 'timeout',
      depth: 3,
    } as never);
    expect(h).toHaveLength(1);
    expect(h[0]).toContain('Depth=3');
    assertLean(h[0]!);
  });
});

describe('limitation hints — silence when threshold not hit', () => {
  it('localSearchCode without size_limit errorType returns []', () => {
    expect(ripgrepHints.error({ errorType: 'other' as never })).toEqual([]);
  });

  it('localGetFileContent without isLarge stays silent', () => {
    expect(
      fetchContentHints.error({
        errorType: 'size_limit',
        fileSize: 100,
      } as never)
    ).toEqual([]);
  });

  it('localViewStructure without entryCount stays silent', () => {
    expect(
      viewStructureHints.error({ errorType: 'size_limit' } as never)
    ).toEqual([]);
  });

  it('lspCallHierarchy unknown errorType returns []', () => {
    expect(callHints.error({ errorType: 'other' as never })).toEqual([]);
  });
});
