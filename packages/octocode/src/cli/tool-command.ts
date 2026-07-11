// Barrel/orchestrator for the `tools` command. The real implementation is
// split by responsibility under ./tool-command/ (registry access,
// input parsing, description formatting, the list/help/catalog views, and
// the main dispatcher) — this file only re-exports the public surface so no
// other file in the repo needs to change its imports.
import './cjs-shim.js';

export type { ToolDefinition } from './tool-command/registry.js';
export {
  TOOL_CATEGORIES,
  TOOL_DEFINITIONS,
  findToolDefinition,
  getToolCategory,
  getDisplayFields,
} from './tool-command/registry.js';

export {
  truncateDescription,
  formatRequiredFields,
} from './tool-command/formatting.js';

export { showAvailableTools } from './tool-command/list-view.js';

export {
  printToolCatalogJson,
  printToolSchemaJson,
} from './tool-command/catalog-json.js';

export { showToolHelp, showMultipleToolSchemas } from './tool-command/help.js';

export {
  getToolsContextString,
  printToolsContext,
} from './tool-command/context.js';

export { executeToolCommand } from './tool-command/execute.js';

export { toolCommand } from './tool-command/command.js';
