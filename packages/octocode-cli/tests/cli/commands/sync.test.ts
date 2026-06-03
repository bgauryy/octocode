import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
  rmSync: vi.fn(),
  statSync: vi.fn(),
  symlinkSync: vi.fn(),
  promises: {
    readFile: vi.fn(),
    writeFile: vi.fn(),
    mkdir: vi.fn(),
    unlink: vi.fn(),
    stat: vi.fn(),
  },
}));

vi.mock('node:crypto', () => ({
  randomBytes: vi.fn().mockReturnValue(Buffer.alloc(32)),
  createCipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('encrypted'),
    final: vi.fn().mockReturnValue(''),
    getAuthTag: vi.fn().mockReturnValue(Buffer.alloc(16)),
  }),
  createDecipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('{}'),
    final: vi.fn().mockReturnValue(''),
    setAuthTag: vi.fn(),
  }),
}));

const featureSyncMocks = vi.hoisted(() => ({
  quickSync: vi.fn(),
  readAllClientConfigs: vi.fn().mockReturnValue([]),
  analyzeSyncState: vi.fn().mockReturnValue({
    clients: [],
    allMCPs: new Set(),
    diffs: [],
    fullyConsistent: [],
    needsSync: [],
    conflicts: [],
    summary: {
      totalClients: 0,
      clientsWithConfig: 0,
      totalUniqueMCPs: 0,
      consistentMCPs: 0,
      needsSyncCount: 0,
      conflictCount: 0,
    },
  }),
  getClientDisplayName: vi.fn().mockImplementation((c: string) => c),
}));

vi.mock('../../../src/features/sync.js', () => ({
  quickSync: featureSyncMocks.quickSync,
  readAllClientConfigs: featureSyncMocks.readAllClientConfigs,
  analyzeSyncState: featureSyncMocks.analyzeSyncState,
  getClientDisplayName: featureSyncMocks.getClientDisplayName,
}));

vi.mock('../../../src/utils/spinner.js', () => ({
  Spinner: vi.fn(function MockSpinner() {
    const instance = {
      start: vi.fn(),
      stop: vi.fn(),
      succeed: vi.fn(),
      fail: vi.fn(),
    };
    instance.start.mockImplementation(() => instance);
    return instance;
  }),
}));

describe('syncCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;

    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.readAllClientConfigs).mockReturnValue([]);
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      allMCPs: new Set(),
      diffs: [],
      fullyConsistent: [],
      needsSync: [],
      conflicts: [],
      summary: {
        totalClients: 0,
        clientsWithConfig: 0,
        totalUniqueMCPs: 0,
        consistentMCPs: 0,
        needsSyncCount: 0,
        conflictCount: 0,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    vi.mocked(features.quickSync).mockReset();
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  async function loadCommand() {
    const mod = await import('../../../src/cli/commands/sync.js');
    return mod.syncCommand;
  }

  it('status: shows clients and MCP summary', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.readAllClientConfigs).mockReturnValue([
      {
        client: 'cursor',
        configPath: '/p',
        config: null,
        exists: true,
        mcpCount: 3,
      },
      {
        client: 'claude-code',
        configPath: '/q',
        config: null,
        exists: false,
        mcpCount: 0,
      },
    ]);
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [
        {
          client: 'cursor',
          configPath: '/p',
          config: null,
          exists: true,
          mcpCount: 3,
        },
      ],
      summary: {
        clientsWithConfig: 2,
        totalUniqueMCPs: 5,
        consistentMCPs: 2,
        needsSyncCount: 3,
        conflictCount: 1,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { status: true },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('MCP Sync Status')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('with MCP configs')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('unique MCPs')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('fully synced')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('can be auto-synced')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('have conflicts')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('octocode sync')
    );
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('--force'));
  });

  it('status: shows dim icon when client config is missing', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [
        {
          client: 'vscode-continue',
          configPath: '/x',
          config: null,
          exists: false,
          mcpCount: 0,
        },
      ],
      summary: {
        clientsWithConfig: 0,
        totalUniqueMCPs: 1,
        consistentMCPs: 0,
        needsSyncCount: 0,
        conflictCount: 0,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { status: true },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('no config')
    );
  });

  it('status: conflict-only summary still suggests --force', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      summary: {
        clientsWithConfig: 2,
        totalUniqueMCPs: 3,
        consistentMCPs: 0,
        needsSyncCount: 0,
        conflictCount: 2,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { status: true },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('auto-resolve conflicts')
    );
  });

  it('status: omits optional summary lines when counts are zero', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      summary: {
        clientsWithConfig: 1,
        totalUniqueMCPs: 2,
        consistentMCPs: 0,
        needsSyncCount: 0,
        conflictCount: 0,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { s: true },
    });

    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('fully synced')
      )
    ).toBe(false);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('can be auto-synced')
      )
    ).toBe(false);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) => String(c[0]).includes('Run'))
    ).toBe(false);
  });

  it('short alias -n triggers dry-run plan (uses analyzeSyncState, not quickSync)', async () => {
    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { n: true },
    });

    expect(featureSyncMocks.analyzeSyncState).toHaveBeenCalled();
    expect(featureSyncMocks.quickSync).not.toHaveBeenCalled();
  });

  it('sync (default): successful sync performed', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: true,
      message: 'Synced!',
      syncPerformed: true,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: {},
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Restart your IDEs')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('sync (default): sync performed but failure sets exit code', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: false,
      message: 'Write failed',
      syncPerformed: true,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: {},
    });

    expect(process.exitCode).toBe(1);
  });

  it('sync (default): already synced shows success message', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: true,
      message: 'All MCPs are already in sync',
      syncPerformed: false,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: {},
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('already in sync')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('sync (default): conflicts without --force prints options', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: false,
      message:
        '2 conflict(s) found. Use --force to auto-resolve or run interactive mode.',
      syncPerformed: false,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: {},
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('conflict')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Options:')
    );
    expect(process.exitCode).toBe(1);
  });

  it('sync (default): conflicts with --force skips conflict hints but still fails', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: false,
      message: 'Still broken after conflict resolution',
      syncPerformed: false,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { force: true },
    });

    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('Options:')
      )
    ).toBe(false);
    expect(process.exitCode).toBe(1);
  });

  it('sync with --force forwards to quickSync', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: true,
      message: 'done',
      syncPerformed: true,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { f: true },
    });

    expect(featureSyncMocks.quickSync).toHaveBeenCalledWith(
      expect.objectContaining({ force: true })
    );
  });

  it('sync with --dry-run uses analyzeSyncState (not quickSync)', async () => {
    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { 'dry-run': true },
    });

    expect(featureSyncMocks.analyzeSyncState).toHaveBeenCalled();
    expect(featureSyncMocks.quickSync).not.toHaveBeenCalled();
  });

  it('status --json outputs structured client/summary report', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.getClientDisplayName).mockImplementation(
      (c: string) => `Name:${c}`
    );
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [
        {
          client: 'cursor',
          configPath: '/p',
          config: null,
          exists: true,
          mcpCount: 2,
        },
      ],
      summary: {
        clientsWithConfig: 1,
        totalUniqueMCPs: 2,
        consistentMCPs: 1,
        needsSyncCount: 0,
        conflictCount: 0,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { status: true, json: true },
    });

    const out = consoleSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.clients[0].client).toBe('cursor');
    expect(parsed.clients[0].name).toBe('Name:cursor');
    expect(parsed.summary.totalUniqueMCPs).toBe(2);
  });

  it('dry-run plan --json outputs operations and summary', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      needsSync: [
        { mcpId: 'add-me', presentIn: ['cursor'], missingIn: ['claude-code'] },
      ],
      conflicts: [
        {
          mcpId: 'conflicted',
          presentIn: ['cursor', 'claude-code'],
          missingIn: [],
        },
      ],
      fullyConsistent: [
        {
          mcpId: 'synced',
          presentIn: ['cursor', 'claude-code'],
          missingIn: [],
        },
      ],
      summary: {
        clientsWithConfig: 2,
        totalUniqueMCPs: 3,
        consistentMCPs: 1,
        needsSyncCount: 1,
        conflictCount: 1,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { 'dry-run': true, json: true },
    });

    const out = consoleSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.dryRun).toBe(true);
    const types = parsed.operations.map((o: { type: string }) => o.type).sort();
    expect(types).toEqual(['add', 'conflict', 'ok']);
  });

  it('dry-run plan (non-json) prints actionable add/conflict lines and hints', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      needsSync: [
        { mcpId: 'add-me', presentIn: ['cursor'], missingIn: ['claude-code'] },
      ],
      conflicts: [
        {
          mcpId: 'conflicted',
          presentIn: ['cursor', 'claude-code'],
          missingIn: [],
        },
      ],
      fullyConsistent: [],
      summary: {
        clientsWithConfig: 2,
        totalUniqueMCPs: 2,
        consistentMCPs: 0,
        needsSyncCount: 1,
        conflictCount: 1,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: ['plan'],
      options: {},
    });

    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('Sync Plan')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('add-me')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('conflicted')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('auto-resolve conflicts')
      )
    ).toBe(true);
  });

  it('dry-run plan (non-json) shows all-in-sync when no actionable ops', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      needsSync: [],
      conflicts: [],
      fullyConsistent: [
        { mcpId: 'synced', presentIn: ['cursor'], missingIn: [] },
      ],
      summary: {
        clientsWithConfig: 1,
        totalUniqueMCPs: 1,
        consistentMCPs: 1,
        needsSyncCount: 0,
        conflictCount: 0,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { 'dry-run': true },
    });

    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('All MCPs are in sync.')
      )
    ).toBe(true);
  });

  it('dry-run plan (non-json) needsSync only prints apply hint without force hint', async () => {
    const features = await import('../../../src/features/sync.js');
    vi.mocked(features.analyzeSyncState).mockReturnValue({
      clients: [],
      needsSync: [
        { mcpId: 'add-me', presentIn: ['cursor'], missingIn: ['claude-code'] },
      ],
      conflicts: [],
      fullyConsistent: [],
      summary: {
        clientsWithConfig: 2,
        totalUniqueMCPs: 1,
        consistentMCPs: 0,
        needsSyncCount: 1,
        conflictCount: 0,
      },
    } as unknown as ReturnType<typeof features.analyzeSyncState>);

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { 'dry-run': true },
    });

    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('to apply.')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('auto-resolve conflicts')
      )
    ).toBe(false);
  });

  it('sync (default) --json outputs result object', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: true,
      message: 'Synced!',
      syncPerformed: true,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { json: true },
    });

    const out = consoleSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(true);
    expect(parsed.syncPerformed).toBe(true);
    expect(parsed.message).toBe('Synced!');
    expect(process.exitCode).toBeUndefined();
  });

  it('sync (default) --json failure sets exit code', async () => {
    featureSyncMocks.quickSync.mockResolvedValue({
      success: false,
      message: 'nope',
      syncPerformed: false,
    });

    const syncCommand = await loadCommand();
    await syncCommand.handler({
      command: 'sync',
      args: [],
      options: { j: true },
    });

    const out = consoleSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(process.exitCode).toBe(1);
  });
});
