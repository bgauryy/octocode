// Machine-readable (`--json`) views: the lean tool catalog, the full
// all-tool schema dump, and a single tool's schema.
import {
  buildDirectToolCommandPatterns,
  formatDirectToolSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolDisplayFields,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import {
  TOOL_DEFINITIONS,
  findToolDefinition,
  getOptionalToolMetadata,
} from './registry.js';
import {
  extractShortDescription,
  formatConciseToolDescription,
  formatRequiredFields,
  formatToolExampleCommand,
  getToolPreviewLines,
  getToolSchemaGuidance,
} from './formatting.js';

type ToolCatalogJsonOptions = {
  full?: boolean;
  compact?: boolean;
};

export function printJsonPayload(payload: unknown, compact = false): void {
  console.log(JSON.stringify(payload, null, compact ? 0 : 2));
}

function formatToolFieldsJson(toolName: string): Array<{
  name: string;
  type: string;
  required: boolean;
  constraints?: string;
  description?: string;
}> {
  return getDirectToolDisplayFields(toolName).map(field => ({
    name: field.name,
    type: field.type,
    required: field.required,
    ...(field.constraints ? { constraints: field.constraints } : {}),
    ...(field.description ? { description: field.description } : {}),
  }));
}

function compactRunCommand(toolName: string): string {
  return `${formatToolExampleCommand(toolName)} --compact`;
}

export async function printToolCatalogJson(
  options: ToolCatalogJsonOptions = {}
): Promise<void> {
  const metadata = await getOptionalToolMetadata();
  const toolNames = sortDirectToolNames(
    TOOL_DEFINITIONS.map(tool => tool.name)
  );

  if (!options.full) {
    const catalog = {
      kind: 'octocode.toolCatalog',
      version: 1,
      toolCount: toolNames.length,
      guidance: [
        'Discovery only: this catalog intentionally omits full schemas.',
        'Read one schema before raw execution: tools <name> --scheme --json.',
        'Run tools with --compact unless you need the full CallToolResult envelope.',
      ],
      commands: {
        list: 'tools --json',
        fullCatalog: 'tools --json --full',
        schema: 'tools <name> --scheme --json',
        humanSchema: 'tools <name> --scheme',
        runCompact: "tools <name> --queries '<json>' --compact",
        runEnvelope: "tools <name> --queries '<json>' --json",
      },
      tools: toolNames.map(toolName => ({
        name: toolName,
        category: getDirectToolCategory(toolName),
        description: formatConciseToolDescription(toolName, metadata),
        fields: formatRequiredFields(toolName),
        ...(getToolPreviewLines(toolName).length > 0
          ? { hints: getToolPreviewLines(toolName) }
          : {}),
        schemaCommand: `tools ${toolName} --scheme --json`,
        runCommand: compactRunCommand(toolName),
      })),
    };

    printJsonPayload(catalog, options.compact);
    return;
  }

  const catalog = {
    kind: 'octocode.toolCatalog.full',
    version: 1,
    toolCount: toolNames.length,
    guidance: [
      'Full all-tool schema catalog. This is intentionally large.',
      'For agent loops prefer tools --json, then tools <name> --scheme --json.',
      'Use this only when automation truly needs every schema in one payload.',
    ],
    commands: {
      list: 'tools --json',
      leanCatalog: 'tools --json --compact',
      schema: 'tools <name> --scheme --json',
      compactSchema: 'tools <name> --scheme --json --compact',
      humanSchema: 'tools <name> --scheme',
      runCompact: "tools <name> --queries '<json>' --compact",
      runEnvelope: "tools <name> --queries '<json>' --json",
    },
    tools: toolNames.map(toolName => {
      const fullDescription = getDirectToolDescription(toolName, metadata);
      const commandPatterns = buildDirectToolCommandPatterns(toolName);

      return {
        name: toolName,
        category: getDirectToolCategory(toolName),
        description: extractShortDescription(fullDescription),
        fullDescription,
        inputSchema: JSON.parse(formatDirectToolSchemaText(toolName)),
        fields: formatToolFieldsJson(toolName),
        ...(getToolSchemaGuidance(toolName).length > 0
          ? { guidance: getToolSchemaGuidance(toolName) }
          : {}),
        autoFilledFields: getDirectToolAutoFilledFields(toolName),
        schemaCommand: `tools ${toolName} --scheme --json`,
        runCommand: compactRunCommand(toolName),
        ...(commandPatterns.length > 0 ? { commandPatterns } : {}),
      };
    }),
  };

  printJsonPayload(catalog, options.compact);
}

export async function printToolSchemaJson(
  toolName: string,
  options: { compact?: boolean } = {}
): Promise<boolean> {
  const tool = findToolDefinition(toolName);
  if (!tool) return false;

  const metadata = await getOptionalToolMetadata();
  const inputSchema = JSON.parse(formatDirectToolSchemaText(tool.name));
  const fullDescription = getDirectToolDescription(tool.name, metadata);
  const commandPatterns = buildDirectToolCommandPatterns(tool.name);
  const autoFilledFields = getDirectToolAutoFilledFields(tool.name);
  const fields = formatToolFieldsJson(tool.name);
  const guidance = getToolSchemaGuidance(tool.name);

  printJsonPayload(
    {
      kind: 'octocode.toolSchema',
      version: 1,
      name: tool.name,
      category: getDirectToolCategory(tool.name),
      description: extractShortDescription(fullDescription),
      inputSchema,
      ...(options.compact
        ? { fieldNames: fields.map(field => field.name) }
        : { fullDescription, fields }),
      ...(guidance.length > 0 ? { guidance } : {}),
      autoFilledFields,
      commands: {
        catalog: 'tools --json',
        schema: `tools ${tool.name} --scheme --json`,
        compactSchema: `tools ${tool.name} --scheme --json --compact`,
        humanSchema: `tools ${tool.name} --scheme`,
        runCompact: compactRunCommand(tool.name),
        runEnvelope: `tools ${tool.name} --queries '<json>' --json`,
      },
      ...(!options.compact && commandPatterns.length > 0
        ? { commandPatterns }
        : {}),
    },
    options.compact
  );
  return true;
}
