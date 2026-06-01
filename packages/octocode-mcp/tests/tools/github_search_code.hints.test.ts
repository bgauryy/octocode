/**
 * Empty-result recovery hints for githubSearchCode.
 * Focus: the path: filter is matched by GitHub against a file's DIRECTORY, not
 * a full file path. The old hint blamed the phrase ("use a single distinctive
 * identifier instead of a phrase"), which is wrong — a single token + a
 * file-pointing path also returns zero. These tests pin the corrected guidance.
 */

import { describe, it, expect } from 'vitest';
import { hints } from '../../src/tools/github_search_code/hints.js';

describe('githubSearchCode empty hints — path: is directory-only', () => {
  it('does NOT blame the phrase when a path filter is present', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      path: 'packages/runtime-core/src',
      keywords: ['const patch'],
    });
    const joined = out.join(' ');
    expect(joined).not.toMatch(
      /single distinctive identifier instead of a phrase/i
    );
  });

  it('explains that path: matches a directory and points to filename:', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      path: 'packages/runtime-core/src',
      keywords: ['createRenderer'],
    });
    const joined = out.join(' ');
    expect(joined).toMatch(/path:/);
    expect(joined).toMatch(/director/i);
    expect(joined).toMatch(/filename:/);
  });

  it('gives phrase-broadening guidance when a phrase is used without a path', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      keywords: ['const patch handler'],
    });
    const joined = out.join(' ');
    // Some actionable broadening guidance, but never the old phrase-blaming line.
    expect(joined).not.toMatch(
      /single distinctive identifier instead of a phrase/i
    );
    expect(joined.length).toBeGreaterThan(0);
  });

  it('still warns that archived repos are under-indexed', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      keywords: ['createRenderer'],
    });
    expect(out.join(' ')).toMatch(/archived/i);
  });
});
