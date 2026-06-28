import { describe, it, expect, afterEach, vi, beforeEach } from 'vitest';
import { statSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { writeFileContent, writeJsonFile } from '../../src/utils/fs.js';
import {
  getOctocodeServerConfig,
  getOctocodeServerConfigWindows,
} from '../../src/utils/mcp-config.js';
import { fetchRawContent } from '../../src/utils/skills-fetch.js';

describe('Finding 3 — writeFileContent uses restrictive permissions', () => {
  const testDir = join(tmpdir(), `octocode-audit-f3-${Date.now()}`);

  afterEach(() => {
    if (existsSync(testDir)) {
      rmSync(testDir, { recursive: true, force: true });
    }
  });

  it('files created with mode 0o600 (owner read/write only)', () => {
    mkdirSync(testDir, { recursive: true });
    const testFile = join(testDir, 'config.json');

    expect(writeFileContent(testFile, '{"token":"secret"}')).toBe(true);

    const mode = statSync(testFile).mode & 0o777;
    expect(mode).toBe(0o600);
  });

  it('no group/other bits set (not world-readable)', () => {
    mkdirSync(testDir, { recursive: true });
    const testFile = join(testDir, 'sensitive.json');
    writeFileContent(testFile, 'sensitive data');

    const mode = statSync(testFile).mode & 0o777;
    expect(mode & 0o077).toBe(0);
  });

  it('writeJsonFile also uses 0o600', () => {
    mkdirSync(testDir, { recursive: true });
    const testFile = join(testDir, 'data.json');

    expect(writeJsonFile(testFile, { key: 'value' })).toBe(true);
    expect(statSync(testFile).mode & 0o777).toBe(0o600);
  });

  it('parent directories created with mode 0o700', () => {
    const nestedFile = join(testDir, 'subdir', 'nested', 'file.txt');
    writeFileContent(nestedFile, 'data');

    const parentMode = statSync(join(testDir, 'subdir')).mode & 0o777;
    expect(parentMode).toBe(0o700);
  });
});

describe('Finding 4 — Direct installer removed (RCE/supply-chain risk)', () => {
  it('direct method throws — curl-pipe pattern is removed', () => {
    expect(() => getOctocodeServerConfig('direct' as any)).toThrow(
      'Unknown install method'
    );
  });

  it('Windows direct method throws — PowerShell Invoke-WebRequest pattern is removed', () => {
    expect(() => getOctocodeServerConfigWindows('direct' as any)).toThrow(
      'Unknown install method'
    );
  });

  it('npx is the only supported method', () => {
    const config = getOctocodeServerConfig('npx');
    expect(config.command).toBe('npx');
    expect(config.args).toEqual(['-y', '@octocodeai/mcp@latest']);
  });
});

describe('Finding 5 — Skills download guardrails', () => {
  const source = {
    id: 'test',
    name: 'Test',
    type: 'github' as const,
    owner: 'test',
    repo: 'test',
    branch: 'main',
    skillsPath: '',
    skillPattern: 'flat-md' as const,
    description: 'test',
    url: 'https://github.com/test/test',
  };

  const originalFetch = global.fetch;

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    global.fetch = originalFetch;
  });

  it('rejects body exceeding MAX_CONTENT_SIZE (2MB > 1MB limit)', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      headers: { get: () => null },
      text: () => Promise.resolve('x'.repeat(2 * 1024 * 1024)),
    });

    await expect(fetchRawContent(source, 'SKILL.md')).rejects.toThrow(
      /Content too large/
    );
  });

  it('rejects when Content-Length header exceeds limit', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      headers: {
        get: (h: string) => (h === 'Content-Length' ? '5000000' : null),
      },
      text: () => Promise.resolve('small'),
    });

    await expect(fetchRawContent(source, 'SKILL.md')).rejects.toThrow(
      /Content too large/
    );
  });

  it('throws on non-OK HTTP response', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      statusText: 'Not Found',
    });

    await expect(fetchRawContent(source, 'SKILL.md')).rejects.toThrow(
      /Failed to fetch/
    );
  });

  it('returns content when within size limits', async () => {
    const content = '# My Skill\nValid content.';
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      headers: { get: () => null },
      text: () => Promise.resolve(content),
    });

    expect(await fetchRawContent(source, 'SKILL.md')).toBe(content);
  });
});
