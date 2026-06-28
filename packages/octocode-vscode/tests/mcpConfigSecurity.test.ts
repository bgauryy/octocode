import * as fsPromises from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, describe, expect, it } from 'vitest';
import { readJsonFile } from '../src/jsonUtils';

const MCP_SERVER_NAME = 'octocode';
const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories
      .splice(0)
      .map(d => fsPromises.rm(d, { recursive: true, force: true }))
  );
});

async function makeTempDir(): Promise<string> {
  const d = await fsPromises.mkdtemp(
    path.join(os.tmpdir(), 'octocode-vscode-sec-')
  );
  temporaryDirectories.push(d);
  return d;
}

type McpServerConfig = {
  command: string;
  type: string;
  args: string[];
  env?: Record<string, string>;
};

type McpConfig = {
  mcpServers: Record<string, McpServerConfig>;
};

async function writeConfig(filePath: string, config: McpConfig): Promise<void> {
  await fsPromises.writeFile(
    filePath,
    JSON.stringify(config, null, 2),
    'utf-8'
  );
}

async function updateMcpConfigToken(
  configPath: string,
  token: string | undefined
): Promise<void> {
  const existingConfig = await readJsonFile<McpConfig>(configPath);
  if (!existingConfig?.mcpServers?.[MCP_SERVER_NAME]) return;

  const serverConfig = existingConfig.mcpServers[MCP_SERVER_NAME];
  if (token) {
    serverConfig.env = { ...serverConfig.env, GITHUB_TOKEN: token };
  } else {
    if (serverConfig.env) {
      delete serverConfig.env.GITHUB_TOKEN;
      if (Object.keys(serverConfig.env).length === 0) {
        delete serverConfig.env;
      }
    }
  }

  await fsPromises.writeFile(
    configPath,
    JSON.stringify(existingConfig, null, 2),
    'utf-8'
  );
}

describe('MCP Config Token Security', () => {
  describe('Token write', () => {
    it('sets GITHUB_TOKEN in server env', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: {
            command: 'npx',
            type: 'stdio',
            args: ['@octocodeai/mcp@latest'],
          },
        },
      });

      await updateMcpConfigToken(configPath, 'ghp_testtoken123');
      const updated = await readJsonFile<McpConfig>(configPath);
      expect(updated?.mcpServers[MCP_SERVER_NAME]?.env?.GITHUB_TOKEN).toBe(
        'ghp_testtoken123'
      );
    });

    it('preserves other env vars when setting token', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: {
            command: 'npx',
            type: 'stdio',
            args: [],
            env: { OTHER_VAR: 'keep-me' },
          },
        },
      });

      await updateMcpConfigToken(configPath, 'new-token');
      const updated = await readJsonFile<McpConfig>(configPath);
      const env = updated?.mcpServers[MCP_SERVER_NAME]?.env;
      expect(env?.OTHER_VAR).toBe('keep-me');
      expect(env?.GITHUB_TOKEN).toBe('new-token');
    });

    it('overwrites existing token', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: {
            command: 'npx',
            type: 'stdio',
            args: [],
            env: { GITHUB_TOKEN: 'old-token' },
          },
        },
      });

      await updateMcpConfigToken(configPath, 'new-token');
      const updated = await readJsonFile<McpConfig>(configPath);
      expect(updated?.mcpServers[MCP_SERVER_NAME]?.env?.GITHUB_TOKEN).toBe(
        'new-token'
      );
    });
  });

  describe('Token removal (logout)', () => {
    it('removes GITHUB_TOKEN from env on logout', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: {
            command: 'npx',
            type: 'stdio',
            args: [],
            env: { GITHUB_TOKEN: 'secret', OTHER: 'keep' },
          },
        },
      });

      await updateMcpConfigToken(configPath, undefined);
      const updated = await readJsonFile<McpConfig>(configPath);
      const env = updated?.mcpServers[MCP_SERVER_NAME]?.env;
      expect(env?.GITHUB_TOKEN).toBeUndefined();
      expect(env?.OTHER).toBe('keep');
    });

    it('removes empty env object after token removal', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: {
            command: 'npx',
            type: 'stdio',
            args: [],
            env: { GITHUB_TOKEN: 'secret' },
          },
        },
      });

      await updateMcpConfigToken(configPath, undefined);
      const updated = await readJsonFile<McpConfig>(configPath);
      expect(updated?.mcpServers[MCP_SERVER_NAME]?.env).toBeUndefined();
    });

    it('does not leave token residue in config file', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: {
            command: 'npx',
            type: 'stdio',
            args: [],
            env: { GITHUB_TOKEN: 'ghp_supersecret' },
          },
        },
      });

      await updateMcpConfigToken(configPath, undefined);
      const raw = await fsPromises.readFile(configPath, 'utf-8');
      expect(raw).not.toContain('ghp_supersecret');
      expect(raw).not.toContain('GITHUB_TOKEN');
    });
  });

  describe('Edge cases', () => {
    it('skips config without octocode server entry', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      const original = {
        mcpServers: { other: { command: 'x', type: 'stdio', args: [] } },
      };
      await writeConfig(configPath, original as McpConfig);

      await updateMcpConfigToken(configPath, 'token');
      const updated = await readJsonFile<McpConfig>(configPath);
      expect(updated?.mcpServers).not.toHaveProperty(MCP_SERVER_NAME);
    });

    it('skips missing config file gracefully', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'nonexistent.json');
      await expect(
        updateMcpConfigToken(configPath, 'token')
      ).resolves.toBeUndefined();
    });

    it('preserves other servers in config', async () => {
      const dir = await makeTempDir();
      const configPath = path.join(dir, 'mcp.json');
      await writeConfig(configPath, {
        mcpServers: {
          [MCP_SERVER_NAME]: { command: 'npx', type: 'stdio', args: [] },
          other: { command: 'other-cmd', type: 'stdio', args: ['--flag'] },
        },
      });

      await updateMcpConfigToken(configPath, 'tok');
      const updated = await readJsonFile<McpConfig>(configPath);
      expect(updated?.mcpServers.other).toBeDefined();
      expect(updated?.mcpServers.other.command).toBe('other-cmd');
    });
  });
});
