import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { McpToolConfig } from './toolConfig.js';
import {
  getServerConfig,
  isLocalEnabled,
  isCloneEnabled,
  logSessionError,
  ignoreBestEffortFailure,
  DEFAULT_TOOL_METADATA_GATEWAY,
} from '@octocodeai/octocode-tools-core';
import type {
  ToolInvocationCallback,
  ToolMetadataGateway,
} from '@octocodeai/octocode-tools-core';
import {
  getToolFilterConfigSafe,
  hasToolFilterConflict,
  isToolEnabled,
  TOOL_FILTER_CONFLICT_WARNING,
} from './toolFilters.js';
import { hasValidMetadata } from './metadataPolicy.js';
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
    metadataGateway?: Pick<ToolMetadataGateway, 'hasTool'>;
  } = {}
): Promise<{
  successCount: number;
  failedTools: string[];
  failedToolErrors?: Record<string, string>;
}> {
  const localEnabled = isLocalEnabled();
  const cloneEnabled = isCloneEnabled();
  const filterConfig = getToolFilterConfigSafe(getServerConfig);
  const metadataGateway =
    options.metadataGateway ?? DEFAULT_TOOL_METADATA_GATEWAY;

  if (hasToolFilterConflict(filterConfig)) {
    process.stderr.write(TOOL_FILTER_CONFLICT_WARNING);
  }

  const secureServer = withOutputSanitization(server);
  const allTools = await loadTools(options.toolLoader);
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
    callback,
    tool =>
      hasValidMetadata(tool, {
        hasTool: metadataGateway.hasTool,
        logSessionErrorSafe,
      })
  );

  return summarizeOutcomes(outcomes);
}

function logSessionErrorSafe(toolName: string, errorCode: string): void {
  try {
    void Promise.resolve(logSessionError(toolName, errorCode)).catch(
      ignoreBestEffortFailure('tool registration session logging')
    );
  } catch {
    void 0;
  }
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
