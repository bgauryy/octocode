import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';

import {
  detectPlatformId,
  executableNames,
  firstExecutableIn,
} from '../../src/lsp/platform.js';

async function withRuntimePlatform(
  platform: NodeJS.Platform,
  arch: NodeJS.Architecture,
  run: (module: typeof import('../../src/lsp/platform.js')) => Promise<void>
): Promise<void> {
  const platformDescriptor = Object.getOwnPropertyDescriptor(process, 'platform')!;
  const archDescriptor = Object.getOwnPropertyDescriptor(process, 'arch')!;
  vi.resetModules();
  Object.defineProperty(process, 'platform', {
    ...platformDescriptor,
    value: platform,
  });
  Object.defineProperty(process, 'arch', { ...archDescriptor, value: arch });
  try {
    await run(await import('../../src/lsp/platform.js'));
  } finally {
    Object.defineProperty(process, 'platform', platformDescriptor);
    Object.defineProperty(process, 'arch', archDescriptor);
    vi.resetModules();
  }
}

describe('platform', () => {
  it('detects a canonical {os}-{arch}[-musl] platform id', () => {
    const id = detectPlatformId();
    expect(id).toMatch(/^(darwin|linux|win32)-(x64|arm64)(-musl)?$/);
    // musl qualifier only ever appears on linux
    if (id.endsWith('-musl')) expect(id.startsWith('linux-')).toBe(true);
  });

  it('expands executable names by PATHEXT only on Windows', () => {
    const names = executableNames('gopls');
    expect(names[0]).toBe('gopls');
    if (process.platform === 'win32') {
      expect(names.length).toBeGreaterThan(1);
      expect(names.some(n => n.toLowerCase().endsWith('.exe'))).toBe(true);
    } else {
      expect(names).toEqual(['gopls']);
    }
  });

  it('returns the first executable candidate and null when none exists', () => {
    const dir = path.dirname(process.execPath);
    expect(
      firstExecutableIn(dir, path.basename(process.execPath), path.join)
    ).toBe(process.execPath);
    expect(
      firstExecutableIn(dir, 'definitely-not-octocode', path.join)
    ).toBeNull();
  });

  it('detects and caches the Linux arm64 runtime variant', async () => {
    await withRuntimePlatform('linux', 'arm64', async platformModule => {
      const id = platformModule.detectPlatformId();
      expect(id).toMatch(/^linux-arm64(?:-musl)?$/);
      expect(platformModule.detectPlatformId()).toBe(id);
    });
  });

  it('detects Windows and normalizes PATHEXT executable candidates', async () => {
    const previousPathExt = process.env.PATHEXT;
    process.env.PATHEXT = '.EXE; ;.CMD';
    try {
      await withRuntimePlatform('win32', 'x64', async platformModule => {
        expect(platformModule.detectPlatformId()).toBe('win32-x64');
        expect(platformModule.executableNames('gopls')).toEqual([
          'gopls',
          'gopls.exe',
          'gopls.cmd',
        ]);
      });
    } finally {
      if (previousPathExt === undefined) delete process.env.PATHEXT;
      else process.env.PATHEXT = previousPathExt;
    }
  });
});
