import {
  discoverMcpSystem as discoverSharedMcpSystem,
  type DiscoverMcpConfigOptions,
  type DiscoveredMcpConfig,
  type McpDiscoveryResult,
} from '../../contracts/agent-skills.js';
import path from 'node:path';
import { capabilitySourcePaths } from '../../contracts/capability-sources.js';
import { extensionWorkspaceRoot, extensionHome } from '../../extension-paths.js';

/** Pi owns its workspace storage policy; shared contracts own discovery and admission. */
export function discoverMcpSystem(cwd: string, options?: string | DiscoverMcpConfigOptions): McpDiscoveryResult {
  const resolvedOptions = typeof options === 'string' ? { homeDir: options } : options;
  const paths = capabilitySourcePaths(cwd, resolvedOptions);
  return discoverSharedMcpSystem(cwd, {
    ...resolvedOptions,
    workspaceRoot: extensionWorkspaceRoot,
    additionalNativeFiles: [...(resolvedOptions?.additionalNativeFiles ?? []), { path: path.join(extensionHome(paths.native.globalRoot), 'mcp', 'servers.json'), scope: 'user' }],
  });
}

export function discoverMcpConfigs(cwd: string, options?: string | DiscoverMcpConfigOptions): DiscoveredMcpConfig[] {
  return discoverMcpSystem(cwd, options).configs;
}
