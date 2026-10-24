import { EventEmitter } from 'events';
import * as fs from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const host = vi.hoisted(() => ({
  commands: new Map<string, (...args: unknown[]) => unknown>(),
  token: vi.fn(),
  info: vi.fn(),
  error: vi.fn(),
  spawn: vi.fn(),
  home: '',
  homeError: undefined as Error | undefined,
  log: [] as string[],
  appName: 'Cursor',
  autoInstall: false,
  sessionChanged: undefined as
    undefined | ((event: { provider: { id: string } }) => Promise<void>),
}));

vi.mock('os', async importOriginal => ({
  ...(await importOriginal<typeof import('os')>()),
  homedir: () => {
    if (host.homeError) throw host.homeError;
    return host.home;
  },
}));
vi.mock('child_process', () => ({ spawn: host.spawn }));
vi.mock('vscode', () => ({
  env: {
    get appName() {
      return host.appName;
    },
  },
  StatusBarAlignment: { Right: 2 },
  authentication: {
    getSession: host.token,
    onDidChangeSessions: (callback: typeof host.sessionChanged) => {
      host.sessionChanged = callback;
      return { dispose() {} };
    },
  },
  workspace: {
    getConfiguration: () => ({
      get: (key: string) =>
        key === 'autoInstallMcp' ? host.autoInstall : undefined,
    }),
  },
  commands: {
    registerCommand: (key: string, callback: () => unknown) => {
      host.commands.set(key, callback);
      return { dispose() {} };
    },
  },
  window: {
    createOutputChannel: () => ({
      appendLine: (line: string) => host.log.push(line),
      dispose() {},
    }),
    createStatusBarItem: () => ({ show() {}, dispose() {} }),
    showInformationMessage: host.info,
    showErrorMessage: host.error,
    showWarningMessage: vi.fn(),
  },
}));

let extension: typeof import('../src/extension');
let directory: string;
const subscriptions: { dispose(): void }[] = [];

async function activate() {
  extension = await import('../src/extension');
  await extension.activate({ subscriptions } as never);
}

async function command(name: string) {
  return host.commands.get(`octocode.${name}`)!();
}

function child() {
  return Object.assign(new EventEmitter(), {
    stdout: new EventEmitter(),
    stderr: new EventEmitter(),
    kill: vi.fn(() => true),
  });
}

beforeEach(async () => {
  vi.resetModules();
  directory = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-extension-'));
  host.home = directory;
  host.homeError = undefined;
  host.log.length = 0;
  vi.stubEnv('APPDATA', path.join(directory, 'AppData'));
  host.appName = 'Cursor';
  host.autoInstall = false;
  host.commands.clear();
  host.token.mockReset().mockResolvedValue(undefined);
  host.spawn.mockReset();
});

afterEach(async () => {
  extension?.deactivate();
  subscriptions.splice(0).forEach(item => item.dispose());
  await fs.rm(directory, { recursive: true, force: true });
  vi.unstubAllEnvs();
});

describe('extension command integration', () => {
  it.each(['Visual Studio Code', 'Code - Insiders'])(
    'installs %s into its own user configuration',
    async appName => {
      host.appName = appName;
      host.autoInstall = true;
      const { getPlatformConfigBase } = await import('../src/configPaths');
      const base = getPlatformConfigBase();
      await activate();
      const product = appName.includes('Insiders') ? 'Code - Insiders' : 'Code';
      const config = JSON.parse(
        await fs.readFile(path.join(base, product, 'User', 'mcp.json'), 'utf8')
      );
      expect(config.servers.octocode.args).toEqual([
        '-y',
        'octocode-mcp@latest',
      ]);
      expect(config).not.toHaveProperty('mcpServers');
      await expect(fs.stat(path.join(base, 'Claude'))).rejects.toMatchObject({
        code: 'ENOENT',
      });
    }
  );

  it('surfaces session token sync failures without rejecting the event callback', async () => {
    const { detectEditorInfo } = await import('../src/configPaths');
    const config = detectEditorInfo(host.appName).mcpConfigPath!;
    await fs.mkdir(path.dirname(config), { recursive: true });
    await fs.writeFile(config, '{ broken');
    await activate();
    await expect(
      host.sessionChanged!({ provider: { id: 'github' } })
    ).resolves.toBeUndefined();
    expect(host.error).toHaveBeenCalledWith(
      expect.stringContaining('GitHub token sync failed')
    );
    expect(await fs.readFile(config, 'utf8')).toBe('{ broken');
  });

  it('logs why the session check failed and falls back to the manual token', async () => {
    host.token.mockRejectedValue(new Error('keychain locked'));
    await activate();
    expect(host.log.join('\n')).toContain('keychain locked');
    expect(host.log.join('\n')).not.toContain('GitHub authenticated');
  });

  it('reports each client whose config path cannot be resolved during token sync', async () => {
    await activate();
    host.homeError = new Error('no home directory');
    host.token.mockResolvedValue({
      accessToken: 'token',
      account: { label: 'octo' },
    });
    await host.sessionChanged!({ provider: { id: 'github' } });
    const log = host.log.join('\n');
    for (const client of ['Cline', 'Roo Code', 'Trae']) {
      expect(log).toContain(`Cannot resolve the ${client} config path`);
    }
    expect(log).toContain('no home directory');
    expect(host.error).toHaveBeenCalledWith(
      expect.stringContaining('Could not update MCP configs for: Cline, Roo Code, Trae')
    );
  });

  it('logs the reason a client is reported failed by installForAll', async () => {
    await activate();
    host.homeError = new Error('no home directory');
    await command('installForAll');
    expect(host.log.join('\n')).toContain('Cannot resolve the Cline config path');
    expect(host.info.mock.calls.at(-1)?.[0]).toContain('❌ Cline (failed)');
  });

  it('leaves invalid configuration untouched and reports failure', async () => {
    const { detectEditorInfo } = await import('../src/configPaths');
    const config = detectEditorInfo(host.appName).mcpConfigPath!;
    await fs.mkdir(path.dirname(config), { recursive: true });
    await fs.writeFile(config, '{ broken');
    await activate();
    await command('installMcp');
    expect(await fs.readFile(config, 'utf8')).toBe('{ broken');
    expect(host.error.mock.calls).toEqual([[expect.stringContaining(config)]]);
  });

  it('reports failed bulk installs instead of already configured', async () => {
    // A file where the platform config directory belongs makes all writes fail.
    const { getPlatformConfigBase } = await import('../src/configPaths');
    const base = getPlatformConfigBase();
    await fs.mkdir(path.dirname(base), { recursive: true });
    await fs.writeFile(base, 'blocked');
    await activate();
    await command('installForAll');
    const summary = host.info.mock.calls.at(-1)?.[0];
    expect(summary).toContain('(failed)');
    expect(summary).not.toContain('(already configured)');
  });

  it('preserves server options when refreshing credentials', async () => {
    const { detectEditorInfo } = await import('../src/configPaths');
    const config = detectEditorInfo(host.appName).mcpConfigPath!;
    await fs.mkdir(path.dirname(config), { recursive: true });
    await fs.writeFile(
      config,
      JSON.stringify({
        mcpServers: {
          octocode: {
            command: 'npx',
            args: ['octocode-mcp@latest'],
            type: 'stdio',
            env: { OTHER: 'keep', GITHUB_TOKEN: 'old' },
            disabled: true,
          },
        },
      })
    );
    await activate();
    await command('installMcp');
    const server = JSON.parse(await fs.readFile(config, 'utf8')).mcpServers
      .octocode;
    expect(server.env).toEqual({ OTHER: 'keep' });
    expect(server.disabled).toBe(true);
    expect(server.args).toEqual(['-y', 'octocode-mcp@latest']);
  });

  it('serializes starts while credentials are loading', async () => {
    await activate();
    let resolve!: (value: undefined) => void;
    host.token.mockReturnValue(
      new Promise(done => {
        resolve = done;
      })
    );
    host.spawn.mockReturnValue(child());
    const first = command('startServer');
    const second = command('startServer');
    resolve(undefined);
    await Promise.all([first, second]);
    await vi.waitFor(() => expect(host.spawn).toHaveBeenCalledTimes(1));
  });

  it('ignores close events from a stopped process after restarting', async () => {
    await activate();
    const first = child();
    const second = child();
    host.spawn.mockReturnValueOnce(first).mockReturnValueOnce(second);
    await command('startServer');
    await vi.waitFor(() => expect(host.spawn).toHaveBeenCalledTimes(1));
    await command('stopServer');
    await command('startServer');
    await vi.waitFor(() => expect(host.spawn).toHaveBeenCalledTimes(2));
    first.emit('close', 0);
    await command('showStatus');
    expect(host.info.mock.calls.at(-1)?.[0]).toContain('server is running.');
    await command('stopServer');
    expect(second.kill).toHaveBeenCalledOnce();
  });
});
