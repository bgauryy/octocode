import type { ToolConfig } from './toolConfig.js';

export interface ToolFilterConfig {
  toolsToRun: string[];
  enableTools: string[];
  disableTools: string[];
}

interface ServerFilterConfigLike {
  toolsToRun?: string[];
  enableTools?: string[];
  disableTools?: string[];
}

export const TOOL_FILTER_CONFLICT_WARNING =
  'Warning: TOOLS_TO_RUN cannot be used together with ENABLE_TOOLS/DISABLE_TOOLS. Using TOOLS_TO_RUN exclusively.\n';

export function getToolFilterConfigSafe(
  configProvider: () => ServerFilterConfigLike
): ToolFilterConfig {
  try {
    const config = configProvider();
    return {
      toolsToRun: config.toolsToRun ?? [],
      enableTools: config.enableTools ?? [],
      disableTools: config.disableTools ?? [],
    };
  } catch {
    return { toolsToRun: [], enableTools: [], disableTools: [] };
  }
}

export function hasToolFilterConflict(config: ToolFilterConfig): boolean {
  return (
    config.toolsToRun.length > 0 &&
    (config.enableTools.length > 0 || config.disableTools.length > 0)
  );
}

export function isToolEnabled(
  tool: ToolConfig,
  options: {
    localEnabled: boolean;
    cloneEnabled: boolean;
    filterConfig: ToolFilterConfig;
  }
): boolean {
  const { localEnabled, cloneEnabled, filterConfig } = options;

  if (tool.isLocal && !localEnabled) {
    return false;
  }

  if (tool.isClone && !cloneEnabled) {
    return false;
  }

  const { toolsToRun, enableTools, disableTools } = filterConfig;

  if (toolsToRun.length > 0) {
    return toolsToRun.includes(tool.name);
  }

  if (disableTools.includes(tool.name)) {
    return false;
  }

  return enableTools.includes(tool.name) || tool.isDefault;
}
