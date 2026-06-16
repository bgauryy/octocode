import { describe, expect, it, vi, afterEach } from 'vitest';
import { join } from 'node:path';

vi.mock('node:fs', async importOriginal => {
  const original = await importOriginal<typeof import('node:fs')>();
  return { ...original, existsSync: vi.fn(original.existsSync) };
});

vi.mock('node:child_process', async importOriginal => {
  const original = await importOriginal<typeof import('node:child_process')>();
  return { ...original, spawnSync: vi.fn(original.spawnSync) };
});

describe('T3.3 — resolveRipgrepBinary (live)', () => {
  it('returns a non-empty string (never undefined)', async () => {
    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const path = resolveRipgrepBinary();
    expect(typeof path).toBe('string');
    expect(path.length).toBeGreaterThan(0);
  });

  it('returns an absolute path, never a bare rg name', async () => {
    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const path = resolveRipgrepBinary();
    expect(path).not.toBe('rg');
    expect(path.startsWith('/') || /^[A-Z]:\\/.test(path)).toBe(true);
  });
});

describe('T3.3 — resolveRipgrepBinary sibling probe', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it('skips sibling probe when running under Node.js runtime', async () => {
    vi.stubGlobal('process', {
      ...process,
      execPath: '/usr/local/bin/node',
      platform: 'linux',
      arch: 'x64',
    });
    const { existsSync: mockExists } = await import('node:fs');
    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    resolveRipgrepBinary();
    const siblingCalls = vi
      .mocked(mockExists)
      .mock.calls.filter(([p]) => String(p).includes('/usr/local/bin/rg'));
    expect(siblingCalls).toHaveLength(0);
  });

  it('uses plain sibling rg when present next to compiled binary', async () => {
    const fakeExecPath = '/usr/local/bin/octocode-mcp-linux-x64';
    const expectedSibling = join('/usr/local/bin', 'rg');

    vi.stubGlobal('process', {
      ...process,
      execPath: fakeExecPath,
      platform: 'linux',
      arch: 'x64',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockImplementation(
      p => String(p) === expectedSibling
    );

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRipgrepBinary();
    expect(result).toBe(expectedSibling);
  });

  it('uses platform-suffixed sibling rg when plain rg absent', async () => {
    const fakeExecPath = '/usr/local/bin/octocode-mcp-darwin-arm64';
    const expectedSibling = join('/usr/local/bin', 'rg-darwin-arm64');

    vi.stubGlobal('process', {
      ...process,
      execPath: fakeExecPath,
      platform: 'darwin',
      arch: 'arm64',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockImplementation(
      p => String(p) === expectedSibling
    );

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRipgrepBinary();
    expect(result).toBe(expectedSibling);
  });

  it('falls back to @vscode/ripgrep when no sibling rg exists', async () => {
    vi.stubGlobal('process', {
      ...process,
      execPath: '/usr/local/bin/octocode-mcp-linux-x64',
      platform: 'linux',
      arch: 'x64',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockReturnValue(false);

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    expect(() => resolveRipgrepBinary()).not.toThrow();
    const result = resolveRipgrepBinary();
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });
});

describe('T3.3 — resolveRipgrepBinary PATH probe', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('resolveRgFromPath returns path when which finds rg', async () => {
    const fakeRgPath = '/opt/homebrew/bin/rg';

    vi.stubGlobal('process', { ...process, platform: 'linux' });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockImplementation(p => String(p) === fakeRgPath);

    const { spawnSync: mockSpawn } = await import('node:child_process');
    vi.mocked(mockSpawn).mockReturnValue({
      status: 0,
      stdout: `${fakeRgPath}\n`,
      stderr: '',
      pid: 1,
      output: [],
      signal: null,
    });

    vi.resetModules();
    const { resolveRgFromPath } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRgFromPath();
    expect(result).toBe(fakeRgPath);
  });

  it('resolveRgFromPath returns null when which fails', async () => {
    vi.stubGlobal('process', { ...process, platform: 'linux' });

    const { spawnSync: mockSpawn } = await import('node:child_process');
    vi.mocked(mockSpawn).mockReturnValue({
      status: 1,
      stdout: '',
      stderr: '',
      pid: 1,
      output: [],
      signal: null,
    });

    vi.resetModules();
    const { resolveRgFromPath } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRgFromPath();
    expect(result).toBeNull();
  });

  it('resolveRgFromPath uses where.exe on Windows', async () => {
    const fakeRgPath = 'C:\\tools\\rg.exe';

    vi.stubGlobal('process', { ...process, platform: 'win32' });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockImplementation(p => String(p) === fakeRgPath);

    const { spawnSync: mockSpawn } = await import('node:child_process');
    vi.mocked(mockSpawn).mockReturnValue({
      status: 0,
      stdout: `${fakeRgPath}\r\n`,
      stderr: '',
      pid: 1,
      output: [],
      signal: null,
    });

    vi.resetModules();
    const { resolveRgFromPath } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRgFromPath();
    expect(result).toBe(fakeRgPath);
    const spawnCalls = vi.mocked(mockSpawn).mock.calls;
    expect(spawnCalls.some(([cmd]) => String(cmd) === 'where.exe')).toBe(true);
  });

  it('covers Homebrew depends_on ripgrep: rg in same bin dir as binary', async () => {
    const homebrewBin = '/opt/homebrew/bin';
    const fakeExecPath = `${homebrewBin}/octocode-mcp`;
    const expectedRg = `${homebrewBin}/rg`;

    vi.stubGlobal('process', {
      ...process,
      execPath: fakeExecPath,
      platform: 'darwin',
      arch: 'arm64',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockImplementation(p => String(p) === expectedRg);

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRipgrepBinary();
    expect(result).toBe(expectedRg);
  });
});

describe('T3.4 — resolved binary passes command allowlist', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it('platform-suffixed sibling binary validates after resolution', async () => {
    const fakeExecPath = '/usr/local/bin/octocode-mcp-darwin-arm64';
    const expectedSibling = join('/usr/local/bin', 'rg-darwin-arm64');

    vi.stubGlobal('process', {
      ...process,
      execPath: fakeExecPath,
      platform: 'darwin',
      arch: 'arm64',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockImplementation(
      p => String(p) === expectedSibling
    );

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const { validateCommand } =
      await import('octocode-security/commandValidator');

    const resolved = resolveRipgrepBinary();
    expect(resolved).toBe(expectedSibling);
    const validation = validateCommand(resolved, ['-n', 'pattern', '.']);
    expect(validation.isValid).toBe(true);
  });

  it('plain rg binary validates without any registration', async () => {
    vi.resetModules();
    const { validateCommand } =
      await import('octocode-security/commandValidator');
    expect(
      validateCommand('/opt/homebrew/bin/rg', ['-n', 'x', '.']).isValid
    ).toBe(true);
  });

  it('never registers a basename that is not shaped like an rg binary', async () => {
    vi.resetModules();
    const { allowRipgrepCommandName } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const { validateCommand } =
      await import('octocode-security/commandValidator');

    allowRipgrepCommandName('/usr/local/bin/ripgrep-custom');
    allowRipgrepCommandName('/usr/local/bin/bash');
    expect(validateCommand('/usr/local/bin/ripgrep-custom', []).isValid).toBe(
      false
    );
    expect(validateCommand('/usr/local/bin/bash', []).isValid).toBe(false);
  });

  it('registers rg flavors for every delivery channel basename', async () => {
    vi.resetModules();
    const { allowRipgrepCommandName } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const { validateCommand } =
      await import('octocode-security/commandValidator');

    allowRipgrepCommandName('/x/dist/runtime/rg/rg-linux-x64');
    allowRipgrepCommandName('C:\\app\\rg-windows-x64.exe');
    expect(validateCommand('/x/dist/runtime/rg/rg-linux-x64', []).isValid).toBe(
      true
    );
    expect(validateCommand('C:\\app\\rg-windows-x64.exe', []).isValid).toBe(
      true
    );
  });
});

describe('T3.3 — ripgrepBinary unknown platform / all-fail branches', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('platformKey returns null for an unsupported platform/arch — no suffixed path probed', async () => {
    vi.stubGlobal('process', {
      ...process,
      execPath: '/usr/local/bin/octocode-mcp-unknown',
      platform: 'freebsd',
      arch: 'mips',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockReturnValue(false);

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    expect(() => resolveRipgrepBinary()).not.toThrow();
  });

  it('throws when all three resolution strategies fail', async () => {
    vi.stubGlobal('process', {
      ...process,
      execPath: '/usr/local/bin/octocode-mcp-unknown',
      platform: 'freebsd',
      arch: 'mips',
    });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockReturnValue(false);

    const { spawnSync: mockSpawn } = await import('node:child_process');
    vi.mocked(mockSpawn).mockReturnValue({
      status: 1,
      stdout: '',
      stderr: '',
      pid: 1,
      output: [],
      signal: null,
    });

    vi.stubEnv('OCTOCODE_DISABLE_VSCODE_RIPGREP', '1');

    vi.resetModules();
    const { resolveRipgrepBinary } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    expect(() => resolveRipgrepBinary()).toThrow(
      /ripgrep \(rg\) is unavailable/
    );
  });

  it('resolveRgFromPath returns null when resolved path does not exist on disk', async () => {
    vi.stubGlobal('process', { ...process, platform: 'linux' });

    const { existsSync: mockExists } = await import('node:fs');
    vi.mocked(mockExists).mockReturnValue(false);

    const { spawnSync: mockSpawn } = await import('node:child_process');
    vi.mocked(mockSpawn).mockReturnValue({
      status: 0,
      stdout: '/nonexistent/path/rg\n',
      stderr: '',
      pid: 1,
      output: [],
      signal: null,
    });

    vi.resetModules();
    const { resolveRgFromPath } =
      await import('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const result = resolveRgFromPath();
    expect(result).toBeNull();
  });
});
