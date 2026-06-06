import * as path from 'path';
import { describe, expect, it } from 'vitest';

import {
  createMcpClients,
  detectEditorInfo,
  getPlatformConfigBase,
} from '../src/configPaths';

describe('getPlatformConfigBase', () => {
  it('returns Application Support on macOS', () => {
    expect(
      getPlatformConfigBase({ platform: 'darwin', homeDir: '/Users/tester' })
    ).toBe('/Users/tester/Library/Application Support');
  });

  it('prefers APPDATA on Windows', () => {
    expect(
      getPlatformConfigBase({
        platform: 'win32',
        homeDir: 'C:\\Users\\tester',
        appData: 'C:\\Users\\tester\\AppData\\Roaming',
      })
    ).toBe('C:\\Users\\tester\\AppData\\Roaming');
  });

  it('falls back to ~/.config on Linux', () => {
    expect(
      getPlatformConfigBase({ platform: 'linux', homeDir: '/home/tester' })
    ).toBe('/home/tester/.config');
  });
});

describe('detectEditorInfo', () => {
  const baseOptions = {
    appData: 'C:\\Users\\tester\\AppData\\Roaming',
    homeDir: '/Users/tester',
    platform: 'darwin' as const,
  };

  it('detects Cursor on macOS', () => {
    expect(detectEditorInfo('Cursor', baseOptions)).toEqual({
      name: 'Cursor',
      scheme: 'cursor',
      mcpConfigPath: '/Users/tester/.cursor/mcp.json',
    });
  });

  it('detects Cursor on Windows with APPDATA', () => {
    expect(
      detectEditorInfo('Cursor', {
        appData: 'C:\\Users\\tester\\AppData\\Roaming',
        homeDir: 'C:\\Users\\tester',
        platform: 'win32',
      })
    ).toEqual({
      name: 'Cursor',
      scheme: 'cursor',
      mcpConfigPath: 'C:\\Users\\tester\\AppData\\Roaming/Cursor/mcp.json',
    });
  });

  it('detects Windsurf', () => {
    expect(detectEditorInfo('Windsurf', baseOptions)).toEqual({
      name: 'Windsurf',
      scheme: 'windsurf',
      mcpConfigPath: '/Users/tester/.codeium/windsurf/mcp_config.json',
    });
  });

  it('falls back to VS Code', () => {
    expect(detectEditorInfo('Code - Insiders', baseOptions)).toEqual({
      name: 'VS Code',
      scheme: 'vscode',
      mcpConfigPath:
        '/Users/tester/Library/Application Support/Claude/claude_desktop_config.json',
    });
  });
});

describe('createMcpClients', () => {
  it('creates platform-aware MCP client paths', () => {
    const clients = createMcpClients({
      homeDir: '/Users/tester',
      platform: 'darwin',
    });

    expect(clients.cline.getConfigPath()).toBe(
      path.join(
        '/Users/tester/Library/Application Support',
        'Code',
        'User',
        'globalStorage',
        'saoudrizwan.claude-dev',
        'settings',
        'cline_mcp_settings.json'
      )
    );
    expect(clients.trae.getConfigPath()).toBe(
      '/Users/tester/Library/Application Support/Trae/mcp.json'
    );
  });
});
