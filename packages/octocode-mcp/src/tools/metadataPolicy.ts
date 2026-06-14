import { TOOL_METADATA_ERRORS } from '@octocodeai/octocode-tools-core';
import type { ToolConfig } from './toolConfig.js';

interface MetadataPolicyDeps {
  hasTool: (toolName: string) => boolean;
  logSessionErrorSafe: (toolName: string, errorCode: string) => void;
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

    deps.logSessionErrorSafe(
      tool.name,
      TOOL_METADATA_ERRORS.INVALID_FORMAT.code
    );
    return false;
  } catch {
    deps.logSessionErrorSafe(
      tool.name,
      TOOL_METADATA_ERRORS.INVALID_API_RESPONSE.code
    );
    return false;
  }
}
