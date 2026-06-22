import type { ToolConfig } from './toolConfig.js';

interface MetadataPolicyDeps {
  hasTool: (toolName: string) => boolean;
}

export function hasValidMetadata(
  tool: ToolConfig,
  deps: MetadataPolicyDeps
): boolean {
  if (tool.skipMetadataCheck) {
    return true;
  }

  try {
    if (deps.hasTool(tool.name)) {
      return true;
    }

    return false;
  } catch {
    return false;
  }
}
