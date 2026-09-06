// Tool registry access: definitions, categories, and lazily-loaded metadata
// (descriptions/system prompt). Kept engine-FREE — it only pulls the
// `/schema` subpath so schema/help/list paths work on runtimes that cannot
// load the native engine (e.g. Codex.app Node).
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  getDirectToolDisplayFields,
  loadToolContent,
  getToolAvailability,
  type DirectToolDefinition,
  type DirectToolDisplayField,
} from '@octocodeai/octocode-tools-core/schema';

export type ToolDefinition = DirectToolDefinition & {
  disabled?: { envVar: string };
};
export const TOOL_CATEGORIES = DIRECT_TOOL_CATEGORIES;

export const TOOL_DEFINITIONS: ToolDefinition[] =
  DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => {
    const availability = getToolAvailability(tool.name);
    return availability.envVar
      ? { ...tool, disabled: { envVar: availability.envVar } }
      : tool;
  });

export function getToolEnableInstruction(toolName: string): string | undefined {
  const availability = getToolAvailability(toolName);
  if (availability.enabled || !availability.envVar) return undefined;
  if (availability.envVar === 'TOOLS_TO_RUN') {
    return `add ${toolName} to TOOLS_TO_RUN`;
  }
  if (availability.envVar === 'DISABLE_TOOLS') {
    return `remove ${toolName} from DISABLE_TOOLS`;
  }
  if (availability.envVar === 'OCTOCODE_STORAGE_MODE') {
    return 'set OCTOCODE_STORAGE_MODE=persistent';
  }
  return `set ${availability.envVar}=true`;
}

let toolMetadataPromise: Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> | null = null;

export function findToolDefinition(name: string): ToolDefinition | undefined {
  return TOOL_DEFINITIONS.find(tool => tool.name === name);
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
