// Tool registry access: definitions, categories, and lazily-loaded metadata
// (descriptions/system prompt). Kept engine-FREE — it only pulls the
// `/schema` subpath so schema/help/list paths work on runtimes that cannot
// load the native engine (e.g. Codex.app Node).
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  findDirectToolDefinition,
  getDirectToolCategory,
  getDirectToolDisplayFields,
  loadToolContent,
  type DirectToolDefinition,
  type DirectToolDisplayField,
} from '@octocodeai/octocode-tools-core/schema';

export type ToolDefinition = DirectToolDefinition;
export const TOOL_CATEGORIES = DIRECT_TOOL_CATEGORIES;
export const TOOL_DEFINITIONS: ToolDefinition[] = DIRECT_TOOL_DEFINITIONS;

let toolMetadataPromise: Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> | null = null;

export function findToolDefinition(name: string): ToolDefinition | undefined {
  return findDirectToolDefinition(name);
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
