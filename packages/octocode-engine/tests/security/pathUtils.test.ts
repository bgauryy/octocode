import os from 'os';
import path from 'path';
import { describe, expect, it } from 'vitest';

import { redactPath } from '../../src/security/pathUtils.js';

describe('redactPath', () => {
  it('returns empty string for empty/falsy input', () => {
    expect(redactPath('')).toBe('');
  });

  it('returns "." when the path equals the workspace root', () => {
    expect(redactPath('/repo/project', '/repo/project')).toBe('.');
  });

  it('returns a path relative to the workspace root for a child path', () => {
    expect(redactPath('/repo/project/src/index.ts', '/repo/project')).toBe(
      'src/index.ts'
    );
  });

  it('normalizes backslashes and traversal before comparing to the root', () => {
    // Windows-style separators plus a redundant traversal collapse to src/a.ts.
    expect(
      redactPath('/repo/project\\src\\sub\\..\\a.ts', '/repo/project')
    ).toBe('src/a.ts');
  });

  it('strips a trailing slash from the normalized root', () => {
    expect(redactPath('/repo/project/file.ts', '/repo/project/')).toBe(
      'file.ts'
    );
  });

  it('renders "~" when the path is exactly the home directory', () => {
    expect(redactPath(os.homedir())).toBe('~');
  });

  it('renders a "~/"-prefixed path for something inside the home directory', () => {
    const p = path.join(os.homedir(), 'Documents', 'notes.txt');
    // Outside the cwd root but inside home → home-relative with ~ prefix.
    expect(redactPath(p, '/some/unrelated/root')).toBe('~/Documents/notes.txt');
  });

  it('falls back to the basename when the path is outside both root and home', () => {
    expect(redactPath('/etc/passwd', '/repo/project')).toBe('passwd');
  });

  it('defaults the root to process.cwd() when workspaceRoot is omitted', () => {
    const p = path.join(process.cwd(), 'nested', 'thing.ts');
    expect(redactPath(p)).toBe('nested/thing.ts');
  });
});
