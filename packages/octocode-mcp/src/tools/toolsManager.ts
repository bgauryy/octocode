import { McpServer } from '@modelcontextprotocol/server';
import type { McpToolConfig } from './toolConfig.js';
import {
  getServerConfig,
  isLocalEnabled,
  isCloneEnabled,
} from '@octocodeai/octocode-tools-core';
import type { ToolInvocationCallback } from '@octocodeai/octocode-tools-core';
import {
  getToolFilterConfigSafe,
  isToolEnabled,
  validateToolFilterConfig,
} from './toolFilters.js';
import { withOutputSanitization } from '../utils/secureServer.js';
import {
  registerToolsBatch,
  summarizeOutcomes,
} from './registrationExecutor.js';

export async function registerTools(
  server: McpServer,
  callback?: ToolInvocationCallback,
  options: {
    toolLoader?: () => Promise<McpToolConfig[]> | McpToolConfig[];
  } = {}
): Promise<{
  successCount: number;
  failedTools: string[];
  failedToolErrors?: Record<string, string>;
}> {
  const localEnabled = isLocalEnabled();
  const cloneEnabled = isCloneEnabled();
  const rawFilterConfig = getToolFilterConfigSafe(getServerConfig);
  const secureServer = withOutputSanitization(server);
  const allTools = await loadTools(options.toolLoader);
  const { config: filterConfig, warnings } = validateToolFilterConfig(
    rawFilterConfig,
    allTools.map(tool => tool.name)
  );
  for (const warning of warnings) process.stderr.write(warning);
  const enabledTools = allTools.filter(tool =>
    isToolEnabled(tool, {
      localEnabled,
      cloneEnabled,
      filterConfig,
    })
  );
  const outcomes = await registerToolsBatch(
    enabledTools,
    secureServer,
    callback
  );

  return summarizeOutcomes(outcomes);
}

async function loadTools(
  injectedLoader?: () => Promise<McpToolConfig[]> | McpToolConfig[]
): Promise<McpToolConfig[]> {
  if (injectedLoader) {
    return Promise.resolve(injectedLoader());
  }

  const { ALL_TOOLS } = await import('./toolConfig.js');
  return ALL_TOOLS;
}
