import { describe, it, expect, afterEach } from 'vitest';
import {
  mkdirSync,
  mkdtempSync,
  writeFileSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { getDirectorySizeBytes, formatBytes } from '../src/fs-utils.js';

const testDir = join(tmpdir(), 'octocode-shared-fs-utils-test');

afterEach(() => {
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    void 0;
  }
});

describe('getDirectorySizeBytes', () => {
  it('returns 0 for non-existent path', () => {
    expect(getDirectorySizeBytes('/non/existent/path')).toBe(0);
  });

  it('returns 0 for empty directory', () => {
    mkdirSync(testDir, { recursive: true });
    expect(getDirectorySizeBytes(testDir)).toBe(0);
  });

  it('sums file sizes in flat directory', () => {
    mkdirSync(testDir, { recursive: true });
    writeFileSync(join(testDir, 'a.txt'), 'hello');
    writeFileSync(join(testDir, 'b.txt'), 'world!');
    expect(getDirectorySizeBytes(testDir)).toBe(11);
  });

  it('recurses into nested directories', () => {
    const nested = join(testDir, 'sub', 'deep');
    mkdirSync(nested, { recursive: true });
    writeFileSync(join(testDir, 'root.txt'), 'abc');
    writeFileSync(join(nested, 'deep.txt'), 'defgh');
    expect(getDirectorySizeBytes(testDir)).toBe(8);
  });

  it('skips symlinks and does not follow circular symlinks (uses lstatSync)', () => {
    const tempDir = mkdtempSync(join(tmpdir(), 'octocode-fs-utils-symlink-'));
    try {
      writeFileSync(join(tempDir, 'a.txt'), 'hello');
      const subDir = join(tempDir, 'sub');
      mkdirSync(subDir, { recursive: true });
      writeFileSync(join(subDir, 'b.txt'), 'world');
      symlinkSync('..', join(subDir, 'loop'));
      const size = getDirectorySizeBytes(tempDir);
      expect(size).toBe(10);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});

describe('formatBytes', () => {
  it('formats bytes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(512)).toBe('512 B');
  });

  it('formats kilobytes', () => {
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1536)).toBe('1.5 KB');
  });

  it('formats megabytes', () => {
    expect(formatBytes(1024 * 1024)).toBe('1.0 MB');
  });

  it('formats gigabytes', () => {
    expect(formatBytes(2 * 1024 * 1024 * 1024)).toBe('2.00 GB');
  });
});
