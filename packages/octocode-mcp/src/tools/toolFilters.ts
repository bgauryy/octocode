import type { ToolConfig } from '@octocodeai/octocode-tools-core';

export interface ToolFilterConfig {
  toolsToRun: string[];
  disableTools: string[];
}

interface ServerFilterConfigLike {
  toolsToRun?: string[];
  disableTools?: string[];
}

export interface ValidatedToolFilterConfig {
  config: ToolFilterConfig;
  warnings: string[];
}

export function getToolFilterConfig(
  configProvider: () => ServerFilterConfigLike
): ToolFilterConfig {
  const config = configProvider();
  return {
    toolsToRun: config.toolsToRun ?? [],
    disableTools: config.disableTools ?? [],
  };
}

function editDistance(left: string, right: string): number {
  const previous = Array.from({ length: right.length + 1 }, (_, i) => i);
  for (let leftIndex = 1; leftIndex <= left.length; leftIndex += 1) {
    const current = [leftIndex];
    for (let rightIndex = 1; rightIndex <= right.length; rightIndex += 1) {
      current[rightIndex] = Math.min(
        (current[rightIndex - 1] ?? 0) + 1,
        (previous[rightIndex] ?? 0) + 1,
        (previous[rightIndex - 1] ?? 0) +
          (left[leftIndex - 1] === right[rightIndex - 1] ? 0 : 1)
      );
    }
    previous.splice(0, previous.length, ...current);
  }
  return previous[right.length] ?? Number.MAX_SAFE_INTEGER;
}

function closestToolName(
  unknownName: string,
  availableNames: readonly string[]
): string | undefined {
  const ranked = availableNames
    .map(name => ({ name, distance: editDistance(unknownName, name) }))
    .sort(
      (left, right) =>
        left.distance - right.distance || left.name.localeCompare(right.name)
    );
  const closest = ranked[0];
  if (!closest) return undefined;
  const threshold = Math.max(3, Math.floor(unknownName.length / 2));
  return closest.distance <= threshold ? closest.name : undefined;
}

function formatUnknownTool(
  envVar: string,
  name: string,
  availableNames: readonly string[]
): string {
  const suggestion = closestToolName(name, availableNames);
  return `[octocode-mcp] Unknown tool name in ${envVar}: ${name}.${
    suggestion ? ` Did you mean "${suggestion}"?` : ''
  }\n`;
}

export function validateToolFilterConfig(
  config: ToolFilterConfig,
  availableNames: readonly string[]
): ValidatedToolFilterConfig {
  const available = new Set(availableNames);
  const validateList = (envVar: string, names: readonly string[]) => {
    const valid: string[] = [];
    const warnings: string[] = [];
    for (const name of names) {
      if (available.has(name)) valid.push(name);
      else warnings.push(formatUnknownTool(envVar, name, availableNames));
    }
    return { valid, warnings };
  };

  const toolsToRun = validateList('TOOLS_TO_RUN', config.toolsToRun);
  const disableTools = validateList('DISABLE_TOOLS', config.disableTools);

  if (config.toolsToRun.length > 0 && toolsToRun.valid.length === 0) {
    throw new Error(toolsToRun.warnings.join('').trim());
  }

  return {
    config: {
      toolsToRun: toolsToRun.valid,
      disableTools: disableTools.valid,
    },
    warnings: [...toolsToRun.warnings, ...disableTools.warnings],
  };
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

  const { toolsToRun, disableTools } = filterConfig;

  if (toolsToRun.length > 0) {
    return toolsToRun.includes(tool.name);
  }

  if (disableTools.includes(tool.name)) {
    return false;
  }

  return tool.isDefault;
}
