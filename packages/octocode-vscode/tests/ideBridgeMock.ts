import { vi } from 'vitest';

export const host = {
  execute: vi.fn(),
  diagnostics: vi.fn(),
  open: vi.fn(),
  languages: vi.fn(),
  appName: 'Test IDE',
  enabled: true as boolean | undefined,
  folders: [] as { uri: { scheme: string; fsPath: string } }[],
  foldersChanged: undefined as undefined | (() => void),
  foldersDispose: vi.fn(),
  configChanged: undefined as
    | undefined
    | ((event: { affectsConfiguration: (key: string) => boolean }) => void),
  configDispose: vi.fn(),
};

export function vscodeMock() {
  class Position {
    constructor(
      readonly line: number,
      readonly character: number
    ) {}
  }
  return {
    Position,
    env: {
      get appName() {
        return host.appName;
      },
    },
    Uri: {
      file: (fsPath: string) => ({
        scheme: 'file',
        fsPath,
        toString: () => `file://${fsPath}`,
      }),
    },
    commands: { executeCommand: host.execute },
    languages: {
      getDiagnostics: host.diagnostics,
      getLanguages: host.languages,
    },
    workspace: {
      get workspaceFolders() {
        return host.folders;
      },
      openTextDocument: host.open,
      onDidChangeWorkspaceFolders: (callback: () => void) => {
        host.foldersChanged = callback;
        return { dispose: host.foldersDispose };
      },
      getConfiguration: () => ({
        get: (_key: string, fallback: boolean) => host.enabled ?? fallback,
      }),
      onDidChangeConfiguration: (callback: typeof host.configChanged) => {
        host.configChanged = callback;
        return { dispose: host.configDispose };
      },
    },
  };
}
