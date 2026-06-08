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

  it('warns that a scoped zero is unreliable for archived AND renamed/redirected repos', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'bgauryy',
      repo: 'octocode-mcp', // redirects to bgauryy/octocode
      keywords: ['signaturesOnly'],
    });
    const joined = out.join(' ');
    expect(joined).toMatch(/archived/i);
    expect(joined).toMatch(/renamed|redirect/i);
    expect(joined).toMatch(/githubGetFileContent/);
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
