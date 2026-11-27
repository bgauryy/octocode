import * as fs from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Extension from '../src/extension';

const host = vi.hoisted(() => ({
  enabled: true,
  log: [] as string[],
  configChanged: undefined as
    | undefined
    | ((event: { affectsConfiguration: (key: string) => boolean }) => void),
}));

vi.mock('vscode', () => ({
  env: { appName: 'Cursor' },
  StatusBarAlignment: { Right: 2 },
  authentication: {
    getSession: vi.fn().mockResolvedValue(undefined),
    onDidChangeSessions: () => ({ dispose() {} }),
  },
  workspace: {
    workspaceFolders: [],
    getConfiguration: () => ({
      get: (key: string) => {
        if (key === 'ideBridge.enabled') return host.enabled;
        return false; // autoInstallMcp off: no client config is written
      },
    }),
    onDidChangeConfiguration: (callback: typeof host.configChanged) => {
      host.configChanged = callback;
      return { dispose() {} };
    },
    onDidChangeWorkspaceFolders: () => ({ dispose() {} }),
  },
  commands: { registerCommand: () => ({ dispose() {} }) },
  window: {
    createOutputChannel: () => ({
      appendLine: (line: string) => host.log.push(line),
      dispose() {},
    }),
    createStatusBarItem: () => ({ show() {}, dispose() {} }),
    showInformationMessage: vi.fn(),
    showErrorMessage: vi.fn(),
    showWarningMessage: vi.fn(),
  },
}));

let extension: typeof Extension;
let directory: string;
const subscriptions: { dispose(): void }[] = [];
const lockFile = () => path.join(directory, 'ide', `${process.pid}.json`);

async function activate() {
  extension = await import('../src/extension');
  await extension.activate({ subscriptions } as never);
}

beforeEach(async () => {
  vi.resetModules();
  directory = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-bridge-ext-'));
  vi.stubEnv('OCTOCODE_HOME', directory);
  host.enabled = true;
  host.log.length = 0;
});

afterEach(async () => {
  await extension?.deactivate();
  subscriptions.splice(0).forEach(item => item.dispose());
  await fs.rm(directory, { recursive: true, force: true });
  vi.unstubAllEnvs();
});

describe('extension IDE bridge wiring', () => {
  it('publishes a lock file while the setting is on and removes it on deactivate', async () => {
    await activate();
    const lock = JSON.parse(await fs.readFile(lockFile(), 'utf8'));
    expect(lock).toMatchObject({
      version: 1,
      pid: process.pid,
      ide: 'Cursor',
    });
    expect(host.log.join('\n')).toContain('IDE bridge listening');
    await extension.deactivate();
    await expect(fs.stat(lockFile())).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('does nothing while the setting is off, and follows it when it changes', async () => {
    host.enabled = false;
    await activate();
    await expect(fs.stat(lockFile())).rejects.toMatchObject({ code: 'ENOENT' });
    host.enabled = true;
    host.configChanged!({ affectsConfiguration: () => true });
    await vi.waitFor(() => fs.stat(lockFile()));
  });

  it('removes the lock file when the extension context is disposed', async () => {
    await activate();
    await fs.stat(lockFile());
    subscriptions.splice(0).forEach(item => item.dispose());
    await vi.waitFor(() =>
      expect(fs.stat(lockFile())).rejects.toMatchObject({ code: 'ENOENT' })
    );
  });

  it('logs a bridge that cannot start and keeps activating', async () => {
    await fs.writeFile(path.join(directory, 'ide'), 'blocked');
    await activate();
    expect(host.log.join('\n')).toContain('IDE bridge failed');
    expect(host.log.join('\n')).toContain('Octocode MCP extension ready.');
  });
});
