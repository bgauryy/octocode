import { describe, it, expect } from 'vitest';
import { processFileContentAPI } from '../../../octocode-tools-core/src/github/fileContentProcess.js';

const SAMPLE = [
  'import React from "react";',
  'export function useState(initialState) {',
  '  const dispatcher = resolveDispatcher();',
  '  return dispatcher.useState(initialState);',
  '}',
  'export function useEffect(create, deps) {',
  '  return dispatcher.useEffect(create, deps);',
  '}',
  'const CONSTANT = "hello";',
].join('\n');

async function run(
  matchString: string,
  opts: {
    isRegex?: boolean;
    caseSensitive?: boolean;
    contextLines?: number;
  } = {}
) {
  return processFileContentAPI(
    SAMPLE,
    'facebook',
    'react',
    'main',
    'src/ReactHooks.js',
    false,
    undefined,
    undefined,
    opts.contextLines ?? 2,
    matchString,
    opts.isRegex,
    opts.caseSensitive
  );
}

describe('processFileContentAPI — matchStringIsRegex', () => {
  it('finds a literal match without isRegex (default behaviour)', async () => {
    const result = await run('useState');
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('useState');
  });

  it('finds a regex match when matchStringIsRegex=true', async () => {
    const result = await run('export function use[A-Z]\\w+', {
      isRegex: true,
    });
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('export function');
  });

  it('returns matchNotFound with a regex-specific hint on regex miss', async () => {
    const result = await run('export function doesNotExist\\w+', {
      isRegex: true,
    });
    expect(result.matchNotFound).toBe(true);
    const hint = (result as { hints?: string[] }).hints?.join(' ') ?? '';
    expect(hint).toMatch(/regex/i);
    expect(hint).not.toMatch(/matchStringIsRegex=true/i);
  });

  it('returns matchNotFound with literal-specific hint on literal miss', async () => {
    const result = await run('DOES_NOT_EXIST_ZZ');
    expect(result.matchNotFound).toBe(true);
    const hint = (result as { hints?: string[] }).hints?.join(' ') ?? '';
    expect(hint).toMatch(/matchStringIsRegex=true/i);
  });

  it('returns matchNotFound with invalid-regex hint when pattern is malformed', async () => {
    const result = await run('[invalid(regex', { isRegex: true });
    expect(result.matchNotFound).toBe(true);
    const hint = (result as { hints?: string[] }).hints?.join(' ') ?? '';
    expect(hint).toMatch(/invalid regex/i);
  });

  it('regex is case-insensitive by default (matches CONSTANT for lowercase)', async () => {
    const result = await run('constant', { isRegex: true });
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('CONSTANT');
  });

  it('regex is case-sensitive when matchStringCaseSensitive=true', async () => {
    const result = await run('constant', {
      isRegex: true,
      caseSensitive: true,
    });
    expect(result.matchNotFound).toBe(true);
  });
});

describe('processFileContentAPI — matchStringCaseSensitive', () => {
  it('case-insensitive literal search finds uppercase match (default)', async () => {
    const result = await run('USESTATE');
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('useState');
  });

  it('case-insensitive literal search finds lowercase match', async () => {
    const result = await run('usestate');
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('useState');
  });

  it('case-sensitive literal search misses when case differs', async () => {
    const result = await run('USESTATE', { caseSensitive: true });
    expect(result.matchNotFound).toBe(true);
    const hint = (result as { hints?: string[] }).hints?.join(' ') ?? '';
    expect(hint).toMatch(/case-sensitive/i);
  });

  it('case-sensitive literal search finds exact case', async () => {
    const result = await run('useState', { caseSensitive: true });
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('useState');
  });

  it('case-sensitive search for CONSTANT finds it (it IS uppercase in source)', async () => {
    const result = await run('CONSTANT', { caseSensitive: true });
    expect(result.matchNotFound).toBeUndefined();
    expect(result.content).toContain('CONSTANT');
  });
});
