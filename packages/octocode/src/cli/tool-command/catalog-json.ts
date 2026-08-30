// Machine-readable (`--json`) views: the lean tool catalog, the full
// all-tool schema dump, and a single tool's schema.
import {
  buildDirectToolCommandPatterns,
  formatDirectToolOutputSchemaText,
  formatDirectToolSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolDisplayFields,
  getDirectToolOutputFields,
  getDirectToolSchemaRelations,
  getDirectToolSchemaVariants,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import {
  TOOL_DEFINITIONS,
  findToolDefinition,
  getOptionalToolMetadata,
  getToolAvailability,
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
  pretty?: boolean;
};

export function printJsonPayload(
  payload: unknown,
  compact = false,
  pretty = false
): void {
  console.log(JSON.stringify(payload, null, compact && !pretty ? 0 : 2));
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

function formatCompactField(
  field: ReturnType<typeof formatToolFieldsJson>[number]
): string {
  const marker = field.required ? '*' : '?';
  return `${field.name}${marker}:${field.type}${field.constraints ? ` ${field.constraints}` : ''}`;
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
      output:
        'results[].{index,status?,meta,data?}; tool payload and continuations are row-local under data',
      commands: {
        fullCatalog: 'tools --json --full',
        schema: 'tools <name> --scheme --json',
        run: "tools <name> --queries '<json>' --compact",
      },
      tools: toolNames.map(toolName => ({
        name: toolName,
        category: getDirectToolCategory(toolName),
        description: formatConciseToolDescription(toolName, metadata),
        fields: formatRequiredFields(toolName),
        availability: getToolAvailability(toolName),
        ...(getToolPreviewLines(toolName).length > 0
          ? { hints: getToolPreviewLines(toolName) }
          : {}),
      })),
    };

    printJsonPayload(catalog, options.compact, options.pretty);
    return;
  }

  const catalog = {
    kind: 'octocode.toolCatalog.full',
    version: 1,
    toolCount: toolNames.length,
    guidance: [
      'Full all-tool schema catalog. This is intentionally large.',
      'For agent loops prefer tools --json --compact, then tools <name> --scheme --json --compact.',
      'Check each tool availability; disabled opt-in tools name the required environment flag.',
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
      const relations = getDirectToolSchemaRelations(toolName);

      return {
        name: toolName,
        category: getDirectToolCategory(toolName),
        description: extractShortDescription(fullDescription),
        fullDescription,
        availability: getToolAvailability(toolName),
        inputSchema: JSON.parse(formatDirectToolSchemaText(toolName)),
        outputSchema: JSON.parse(formatDirectToolOutputSchemaText(toolName)),
        fields: formatToolFieldsJson(toolName),
        ...(relations.length > 0 ? { relations } : {}),
        ...(getDirectToolSchemaVariants(toolName).length > 0
          ? { variants: getDirectToolSchemaVariants(toolName) }
          : {}),
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

  printJsonPayload(catalog, options.compact, options.pretty);
}

export async function printToolSchemaJson(
  toolName: string,
  options: { compact?: boolean; pretty?: boolean } = {}
): Promise<boolean> {
  const payload = await buildToolSchemaJson(toolName, options);
  if (!payload) return false;
  printJsonPayload(payload, options.compact === true, options.pretty === true);
  return true;
}

export async function printMultipleToolSchemasJson(
  toolNames: string[],
  options: { compact?: boolean; pretty?: boolean } = {}
): Promise<boolean> {
  const schemas = await Promise.all(
    toolNames.map(toolName => buildToolSchemaJson(toolName, options))
  );
  if (schemas.some(schema => schema === undefined)) return false;
  printJsonPayload(
    {
      kind: options.compact
        ? 'octocode.toolSchemas.compact'
        : 'octocode.toolSchemas',
      version: 1,
      schemas,
    },
    options.compact === true,
    options.pretty === true
  );
  return true;
}

async function buildToolSchemaJson(
  toolName: string,
  options: { compact?: boolean } = {}
): Promise<Record<string, unknown> | undefined> {
  const tool = findToolDefinition(toolName);
  if (!tool) return undefined;

  const metadata = await getOptionalToolMetadata();
  const fullDescription = getDirectToolDescription(tool.name, metadata);
  const fields = formatToolFieldsJson(tool.name);
  const guidance = getToolSchemaGuidance(tool.name);
  const relations = getDirectToolSchemaRelations(tool.name);
  const variants = getDirectToolSchemaVariants(tool.name);

  if (options.compact) {
    return {
      kind: 'octocode.toolSchema.compact',
      version: 1,
      name: tool.name,
      category: getDirectToolCategory(tool.name),
      description: formatConciseToolDescription(tool.name, metadata, 160),
      availability: getToolAvailability(tool.name),
      fields: fields.map(formatCompactField),
      output: getDirectToolOutputFields(tool.name),
      ...(relations.length > 0 ? { relations } : {}),
      ...(variants.length > 0 ? { variants } : {}),
      ...(guidance.length > 0 ? { guidance } : {}),
      commands: {
        full: `tools ${tool.name} --scheme --json`,
        run: compactRunCommand(tool.name),
      },
    };
  }

  const inputSchema = JSON.parse(formatDirectToolSchemaText(tool.name));
  const outputSchema = JSON.parse(formatDirectToolOutputSchemaText(tool.name));
  const commandPatterns = buildDirectToolCommandPatterns(tool.name);
  const autoFilledFields = getDirectToolAutoFilledFields(tool.name);

  return {
    kind: 'octocode.toolSchema',
    version: 1,
    name: tool.name,
    category: getDirectToolCategory(tool.name),
    description: extractShortDescription(fullDescription),
    inputSchema,
    outputSchema,
    fullDescription,
    availability: getToolAvailability(tool.name),
    fields,
    ...(relations.length > 0 ? { relations } : {}),
    ...(variants.length > 0 ? { variants } : {}),
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
    ...(commandPatterns.length > 0 ? { commandPatterns } : {}),
  };
}
