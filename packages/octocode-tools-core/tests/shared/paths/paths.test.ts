import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { existsSync, mkdirSync } from 'node:fs';

vi.mock('node:fs', () => ({
  existsSync: vi.fn(),
  mkdirSync: vi.fn(),
}));

function mockPlatform(platform: string, home: string): void {
  vi.doMock('node:os', () => ({
    default: {
      platform: () => platform,
      homedir: () => home,
      arch: () => 'x64',
    },
    platform: () => platform,
    homedir: () => home,
    arch: () => 'x64',
  }));
}

describe('paths', () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(existsSync).mockReturnValue(false);
    vi.resetModules();
    process.env = { ...originalEnv };
  });

  afterEach(() => {
    process.env = { ...originalEnv };
    vi.doUnmock('node:os');
  });

  it('uses ~/.octocode on macOS', async () => {
    mockPlatform('darwin', '/Users/tester');
    const mod = await import('../../../src/shared/paths.js');

    expect(mod.getDefaultOctocodeHome()).toBe('/Users/tester/.octocode');
    expect(mod.paths.home).toBe('/Users/tester/.octocode');
    expect(mod.paths.config).toBe('/Users/tester/.octocode/.octocoderc');
    expect(mod.paths.credentials).toBe(
      '/Users/tester/.octocode/credentials.json'
    );
    expect(mod.paths.tmp).toBe('/Users/tester/.octocode/tmp');
    expect(mod.paths.clone).toBe('/Users/tester/.octocode/tmp/clone');
    expect(mod.paths.tree).toBe('/Users/tester/.octocode/tmp/tree');
    expect(mod.paths.binary).toBe('/Users/tester/.octocode/tmp/binary');
    expect(mod.paths.repos).toBe('/Users/tester/.octocode/tmp/clone');
    expect(mod.paths.unzip).toBe('/Users/tester/.octocode/tmp/unzip');
    expect(mod.paths.lspConfig).toBe(
      '/Users/tester/.octocode/lsp-servers.json'
    );
  });

  it('uses %APPDATA%\\.octocode on Windows', async () => {
    mockPlatform('win32', 'C:\\Users\\TestUser');
    process.env.APPDATA = 'C:\\Users\\TestUser\\AppData\\Roaming';
    const mod = await import('../../../src/shared/paths.js');

    expect(mod.paths.home.replaceAll('\\', '/')).toBe(
      'C:/Users/TestUser/AppData/Roaming/.octocode'
    );
  });

  it('uses ${XDG_CONFIG_HOME}/.octocode on Linux when available', async () => {
    mockPlatform('linux', '/home/tester');
    process.env.XDG_CONFIG_HOME = '/xdg/config';
    const mod = await import('../../../src/shared/paths.js');

    expect(mod.paths.home).toBe('/xdg/config/.octocode');
  });

  it('uses ~/.config/.octocode on Linux when XDG_CONFIG_HOME is unset', async () => {
    mockPlatform('linux', '/home/tester');
    delete process.env.XDG_CONFIG_HOME;
    const mod = await import('../../../src/shared/paths.js');

    expect(mod.paths.home).toBe('/home/tester/.config/.octocode');
  });

  it('honors OCTOCODE_HOME for isolated test and agent caches', async () => {
    mockPlatform('darwin', '/Users/tester');
    process.env.OCTOCODE_HOME = '/tmp/custom-octocode-home';
    const mod = await import('../../../src/shared/paths.js');

    expect(mod.paths.home).toBe('/tmp/custom-octocode-home');
    expect(mod.getDefaultOctocodeHome()).toBe('/tmp/custom-octocode-home');
    expect(mod.paths.binary).toBe('/tmp/custom-octocode-home/tmp/binary');
  });

  it('ensureHome creates home with 0o700', async () => {
    const mod = await import('../../../src/shared/paths.js');
    mod.ensureHome();

    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.home, {
      recursive: true,
      mode: 0o700,
    });
  });

  it('ensureRepos creates clone tmp with 0o700', async () => {
    const mod = await import('../../../src/shared/paths.js');
    mod.ensureRepos();

    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.home, {
      recursive: true,
      mode: 0o700,
    });
    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.tmp, {
      recursive: true,
      mode: 0o700,
    });
    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.clone, {
      recursive: true,
      mode: 0o700,
    });
  });

  it('ensureTree creates tree tmp with 0o700', async () => {
    const mod = await import('../../../src/shared/paths.js');
    mod.ensureTree();

    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.tmp, {
      recursive: true,
      mode: 0o700,
    });
    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.tree, {
      recursive: true,
      mode: 0o700,
    });
  });

  it('ensureBinary creates binary tmp with 0o700', async () => {
    const mod = await import('../../../src/shared/paths.js');
    mod.ensureBinary();

    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.tmp, {
      recursive: true,
      mode: 0o700,
    });
    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.binary, {
      recursive: true,
      mode: 0o700,
    });
  });

  it('ensureHome does NOT call mkdirSync when directory already exists', async () => {
    vi.mocked(existsSync).mockReturnValue(true);
    const mod = await import('../../../src/shared/paths.js');
    mod.ensureHome();

    expect(mkdirSync).not.toHaveBeenCalled();
  });

  it('ensureHome DOES call mkdirSync when directory does not exist', async () => {
    vi.mocked(existsSync).mockReturnValue(false);
    const mod = await import('../../../src/shared/paths.js');
    mod.ensureHome();

    expect(mkdirSync).toHaveBeenCalledWith(mod.paths.home, {
      recursive: true,
      mode: 0o700,
    });
  });
});
