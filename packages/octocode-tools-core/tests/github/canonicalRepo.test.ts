import { describe, expect, it, vi } from 'vitest';
import { resolveCanonicalOwnerRepo } from '../../src/github/canonicalRepo.js';

function makeOctokit(fullName?: string, shouldThrow = false) {
  return {
    rest: {
      repos: {
        get: vi.fn().mockImplementation(async () => {
          if (shouldThrow) throw new Error('Not Found');
          return {
            data: { full_name: fullName ?? 'facebook/react' },
            status: 200,
          };
        }),
      },
    },
  } as never;
}

describe('resolveCanonicalOwnerRepo', () => {
  it('returns canonical owner/repo from full_name', async () => {
    const octokit = makeOctokit('facebook/react');
    const result = await resolveCanonicalOwnerRepo(
      octokit,
      'facebook',
      'react'
    );
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
    expect(result.renamed).toBe(false);
  });

  it('detects a rename when full_name differs from inputs', async () => {
    const octokit = makeOctokit('meta/react');
    const result = await resolveCanonicalOwnerRepo(
      octokit,
      'facebook',
      'react'
    );
    expect(result.owner).toBe('meta');
    expect(result.repo).toBe('react');
    expect(result.renamed).toBe(true);
  });

  it('detects a repo rename', async () => {
    const octokit = makeOctokit('facebook/react-v2');
    const result = await resolveCanonicalOwnerRepo(
      octokit,
      'facebook',
      'react'
    );
    expect(result.renamed).toBe(true);
    expect(result.repo).toBe('react-v2');
  });

  it('degrades gracefully on API error and returns original owner/repo', async () => {
    const octokit = makeOctokit(undefined, true);
    const result = await resolveCanonicalOwnerRepo(
      octokit,
      'facebook',
      'react'
    );
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
    expect(result.renamed).toBe(false);
  });

  it('returns original values when full_name is empty string', async () => {
    const octokit = {
      rest: {
        repos: {
          get: vi.fn().mockResolvedValue({ data: { full_name: '' } }),
        },
      },
    } as never;
    const result = await resolveCanonicalOwnerRepo(
      octokit,
      'facebook',
      'react'
    );
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
    expect(result.renamed).toBe(false);
  });

  it('returns original values when full_name has no slash', async () => {
    const octokit = {
      rest: {
        repos: {
          get: vi.fn().mockResolvedValue({ data: { full_name: 'onlyone' } }),
        },
      },
    } as never;
    const result = await resolveCanonicalOwnerRepo(
      octokit,
      'facebook',
      'react'
    );
    // split('/') gives ['onlyone'], so canonicalRepo is undefined → return original
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
  });
});
