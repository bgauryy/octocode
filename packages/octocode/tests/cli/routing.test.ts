import { describe, it, expect } from 'vitest';
import path from 'node:path';
import {
  resolveRef,
  isGithubRef,
  isLocalRef,
  refLabel,
  type GithubRef,
} from '../../src/cli/routing.js';

function gh(input: string): GithubRef {
  const ref = resolveRef(input);
  if (!isGithubRef(ref)) {
    throw new Error(`expected a GitHub ref for "${input}", got ${ref.kind}`);
  }
  return ref;
}

describe('resolveRef — GitHub ref parsing', () => {
  it('parses owner/repo with no subpath or branch', () => {
    const ref = gh('facebook/react');
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: '',
    });
    expect(ref.branch).toBeUndefined();
  });

  it('parses owner/repo/path (sparse subtree, no branch)', () => {
    const ref = gh('facebook/react/packages/react');
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: 'packages/react',
    });
    expect(ref.branch).toBeUndefined();
  });

  it('parses owner/repo@branch (branch, no subpath)', () => {
    const ref = gh('facebook/react@main');
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: '',
      branch: 'main',
    });
  });

  it('parses owner/repo@branch/path (branch before path)', () => {
    const ref = gh('facebook/react@main/packages/react');
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: 'packages/react',
      branch: 'main',
    });
  });

  // Regression: the form emitted by refLabel — branch trailing after the
  // subpath. Previously
  // this mis-parsed "path@branch" into the subpath with no branch.
  it('parses owner/repo/path@branch (trailing branch)', () => {
    const ref = gh('facebook/react/packages/react@main');
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: 'packages/react',
      branch: 'main',
    });
  });

  it('parses GitHub tree URLs', () => {
    const ref = gh(
      'https://github.com/facebook/react/tree/main/packages/react'
    );
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: 'packages/react',
      branch: 'main',
    });
  });

  // Regression: a path-shaped miss whose LEADING segment is an existing local
  // directory is almost certainly a local path with a typo — route it local so
  // the user gets a precise "file not found" instead of a confusing GitHub
  // "repository not found (owner=src...)". `src` exists in the package cwd.
  it('routes a path-shaped miss under an existing local dir to local', () => {
    const ref = resolveRef('src/does/not/exist.ts');
    expect(isLocalRef(ref)).toBe(true);
    if (isLocalRef(ref)) {
      expect(ref.path).toBe(path.resolve('src/does/not/exist.ts'));
    }
  });

  // A genuine owner/repo shorthand has a leading segment that does NOT exist on
  // disk, so it must still route to GitHub even when it ends in a file ext.
  it('still routes owner/repo/path.ext (no matching local dir) to GitHub', () => {
    const ref = gh('facebook/react/packages/react/index.js');
    expect(ref).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      subpath: 'packages/react/index.js',
    });
  });

  it('round-trips refLabel output back through the parser', () => {
    const original = gh('facebook/react/packages/react@main');
    const reparsed = gh(refLabel(original));
    expect(reparsed).toMatchObject({
      owner: original.owner,
      repo: original.repo,
      subpath: original.subpath,
      branch: original.branch,
    });
  });
});
