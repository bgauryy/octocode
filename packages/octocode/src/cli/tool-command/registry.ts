// Tool registry access: definitions, categories, and lazily-loaded metadata
// (descriptions/system prompt). Kept engine-FREE — it only pulls the
// `/schema` subpath so schema/help/list paths work on runtimes that cannot
// load the native engine (e.g. Codex.app Node).
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  getDirectToolCategory,
  getDirectToolDisplayFields,
  loadToolContent,
  STATIC_TOOL_NAMES,
  type DirectToolDefinition,
  type DirectToolDisplayField,
} from '@octocodeai/octocode-tools-core/schema';
import { getConfigSync } from '@octocodeai/config';

export type ToolDefinition = DirectToolDefinition & {
  disabled?: { envVar: string };
};
export const TOOL_CATEGORIES = DIRECT_TOOL_CATEGORIES;

function isCloneEnabled(): boolean {
  try {
    return getConfigSync().local.enableClone;
  } catch {
    return true;
  }
}

const cloneEnabled = isCloneEnabled();
const toolConfig = getConfigSync().tools;
const explicitlyEnabledTools = new Set(toolConfig.enabled ?? []);
const explicitlyDisabledTools = new Set(toolConfig.disabled ?? []);
const hasToolAllowlist = explicitlyEnabledTools.size > 0;

export const TOOL_DEFINITIONS: ToolDefinition[] =
  DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => {
    if (tool.name === STATIC_TOOL_NAMES.GITHUB_CLONE_REPO && !cloneEnabled) {
      return { ...tool, disabled: { envVar: 'ENABLE_CLONE' } };
    }
    if (hasToolAllowlist && !explicitlyEnabledTools.has(tool.name)) {
      return { ...tool, disabled: { envVar: 'TOOLS_TO_RUN' } };
    }
    if (!hasToolAllowlist && explicitlyDisabledTools.has(tool.name)) {
      return { ...tool, disabled: { envVar: 'DISABLE_TOOLS' } };
    }
    return tool;
  });

export function getToolAvailability(toolName: string): {
  enabled: boolean;
  envVar?: string;
} {
  const tool = findToolDefinition(toolName);
  return tool?.disabled
    ? { enabled: false, envVar: tool.disabled.envVar }
    : { enabled: true };
}

export function getToolEnableInstruction(toolName: string): string | undefined {
  const availability = getToolAvailability(toolName);
  if (availability.enabled || !availability.envVar) return undefined;
  if (availability.envVar === 'TOOLS_TO_RUN') {
    return `add ${toolName} to TOOLS_TO_RUN`;
  }
  if (availability.envVar === 'DISABLE_TOOLS') {
    return `remove ${toolName} from DISABLE_TOOLS`;
  }
  return `set ${availability.envVar}=true`;
}

let toolMetadataPromise: Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> | null = null;

export function findToolDefinition(name: string): ToolDefinition | undefined {
  return TOOL_DEFINITIONS.find(tool => tool.name === name);
}

export function getToolCategory(
  toolName: string
): ReturnType<typeof getDirectToolCategory> {
  return getDirectToolCategory(toolName);
}

export function getDisplayFields(
  tool: ToolDefinition
): DirectToolDisplayField[] {
  return getDirectToolDisplayFields(tool.name);
}

export async function loadToolMetadata(): Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> {
  if (!toolMetadataPromise) {
    toolMetadataPromise = loadToolContent();
  }

  return toolMetadataPromise;
}

export async function getOptionalToolMetadata(): Promise<Awaited<
  ReturnType<typeof loadToolContent>
> | null> {
  try {
    return await loadToolMetadata();
  } catch {
    return null;
  }
}
