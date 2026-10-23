import type { EditorInfo, McpClientDef } from './configPaths';
import {
  configureMcpServer,
  updateMcpConfigToken,
  type ConfigKey,
} from './mcpConfig';

type InstallerEvents = {
  clients: Record<string, McpClientDef>;
  editorInfo(): EditorInfo;
  getToken(): Promise<string | undefined>;
  output(message: string): void;
  info(message: string): void;
  error(message: string): void;
};

export type InstallTarget = {
  name: string;
  path: string;
  configKey: ConfigKey;
};
export type InstallResult = 'installed' | 'unchanged' | 'failed';

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function clientTarget(client: McpClientDef, path: string): InstallTarget {
  return { name: client.name, path, configKey: client.configKey };
}

/**
 * Writes the Octocode server entry, and the GitHub token in its env block, into
 * the running editor's MCP config and each supported client's.
 */
export class McpInstaller {
  constructor(private readonly events: InstallerEvents) {}

  /** A client's config path, or undefined after logging why it cannot be resolved (the platform's home directory is unreadable). */
  configPath(client: McpClientDef): string | undefined {
    try {
      return client.getConfigPath();
    } catch (err) {
      this.events.output(
        `Cannot resolve the ${client.name} config path: ${String(err)}`
      );
      return undefined;
    }
  }

  async syncToken(token: string | undefined): Promise<void> {
    const editorInfo = this.events.editorInfo();
    const targets: InstallTarget[] = [];
    const failures: string[] = [];

    if (editorInfo.mcpConfigPath) {
      targets.push({
        name: editorInfo.name,
        path: editorInfo.mcpConfigPath,
        configKey: editorInfo.configKey,
      });
    }

    for (const client of Object.values(this.events.clients)) {
      const clientPath = this.configPath(client);
      if (clientPath === undefined) {
        failures.push(client.name);
        continue;
      }
      targets.push(clientTarget(client, clientPath));
    }

    const seen = new Set<string>();
    for (const { name, path: configPath, configKey } of targets) {
      if (seen.has(configPath)) continue;
      seen.add(configPath);
      try {
        await updateMcpConfigToken(configPath, token, configKey);
        this.events.output(`Updated token in ${name} config: ${configPath}`);
      } catch (err) {
        this.events.output(`Failed to update ${name} config: ${String(err)}`);
        failures.push(name);
      }
    }
    if (failures.length)
      throw new Error(
        `Could not update MCP configs for: ${failures.join(', ')}`
      );
  }

  async install(
    target: InstallTarget,
    notify: boolean
  ): Promise<InstallResult> {
    try {
      const result = await configureMcpServer(
        target.path,
        await this.events.getToken(),
        target.configKey
      );
      this.events.output(`MCP config ${result}: ${target.path}`);
      if (notify) {
        this.events.info(
          result === 'installed'
            ? `Octocode MCP server configured for ${target.name}! Restart to enable it.`
            : `Octocode MCP server is already configured for ${target.name}.`
        );
      }
      return result;
    } catch (err) {
      const message = errorMessage(err);
      this.events.output(`Failed to configure MCP server: ${message}`);
      if (notify)
        this.events.error(`Failed to configure MCP server: ${message}`);
      return 'failed';
    }
  }

  async installForEditor(editorInfo: EditorInfo): Promise<void> {
    try {
      if (editorInfo.mcpConfigPath) {
        await this.install(
          {
            name: editorInfo.name,
            path: editorInfo.mcpConfigPath,
            configKey: editorInfo.configKey,
          },
          true
        );
      } else {
        this.events.error('MCP configuration not supported for this editor.');
      }
    } catch (err) {
      this.events.error(`Failed to install MCP: ${errorMessage(err)}`);
    }
  }

  async installForClient(clientKey: string): Promise<void> {
    try {
      const client = this.events.clients[clientKey];
      if (!client) {
        this.events.error(`Unknown MCP client: ${clientKey}`);
        return;
      }

      await this.install(clientTarget(client, client.getConfigPath()), true);
    } catch (err) {
      const errorMsg = errorMessage(err);
      this.events.error(`Error installing for ${clientKey}: ${errorMsg}`);
      this.events.output(`Error installing for ${clientKey}: ${errorMsg}`);
    }
  }

  async installForAll(): Promise<void> {
    const results: string[] = [];
    for (const client of Object.values(this.events.clients)) {
      const configPath = this.configPath(client);
      const installed =
        configPath === undefined
          ? 'failed'
          : await this.install(clientTarget(client, configPath), false);
      if (installed === 'installed') {
        results.push(`✅ ${client.name}`);
      } else {
        results.push(
          installed === 'unchanged'
            ? `⏭️ ${client.name} (already configured)`
            : `❌ ${client.name} (failed)`
        );
      }
    }
    this.events.info(
      `Octocode MCP installation complete:\n${results.join('\n')}`
    );
  }
}
