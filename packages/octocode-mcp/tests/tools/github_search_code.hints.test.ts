import { describe, it, expect } from 'vitest';
import { hints } from '../../../octocode-tools-core/src/tools/github_search_code/hints.js';

describe('ghSearchCode empty hints — path: is directory-only', () => {
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

  it('includes ghGetFileContent fallback for scoped zero results', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'bgauryy',
      repo: 'octocode-mcp',
      keywords: ['extractSignatures'],
    });
    const joined = out.join(' ');
    expect(joined).toMatch(/ghGetFileContent/);
  });

  it('with path filter returns filter removal hint', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      path: 'packages/runtime-core/src',
      keywords: ['createRenderer'],
    });
    const joined = out.join(' ');
    expect(joined).toContain('Remove path/filename/extension first');
  });

  it('gives broadening guidance when no path filter is set', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      keywords: ['const patch handler'],
    });
    const joined = out.join(' ');
    expect(joined.length).toBeGreaterThan(0);
    expect(joined).not.toMatch(
      /single distinctive identifier instead of a phrase/i
    );
  });

  it('includes ghGetFileContent fallback hint for repos without path filter', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'vuejs',
      repo: 'core',
      keywords: ['createRenderer'],
    });
    expect(out.join(' ')).toMatch(/ghGetFileContent/);
  });
});

describe('ghSearchCode empty hints — path filter', () => {
  it('with file-extension path returns filter removal hint', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'mastra-ai',
      repo: 'mastra',
      path: 'packages/core/src/agent/agent.ts',
      keywords: ['createAgent'],
    });
    expect(out.join(' ')).toContain('Remove path/filename/extension first');
  });

  it('with file path returns repo scope in hint', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'facebook',
      repo: 'react',
      path: 'packages/react/src/ReactHooks.js',
      keywords: ['useState'],
    });
    const joined = out.join(' ');
    expect(joined).toContain('Remove path/filename/extension first');
  });

  it('with directory path returns filter hint and ghGetFileContent fallback', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'mastra-ai',
      repo: 'mastra',
      path: 'packages/core/src/agent',
      keywords: ['createAgent'],
    });
    const joined = out.join(' ');
    expect(joined).toContain('Remove path/filename/extension first');
    expect(joined).not.toMatch(/auto-extracted/i);
  });

  it('does NOT fire auto-extraction hint when explicit filename is provided', () => {
    const out = hints.empty({
      hasOwnerRepo: true,
      owner: 'mastra-ai',
      repo: 'mastra',
      path: 'packages/core/src/agent',
      filename: 'agent.ts',
      keywords: ['createAgent'],
    });
    const joined = out.join(' ');
    expect(joined).not.toMatch(/auto-extracted/i);
  });
});
