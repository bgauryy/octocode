// Machine-readable (`--json`) views: the lean tool catalog, the full
// all-tool schema dump, and a single tool's schema.
import {
  buildDirectToolCommandPatterns,
  formatDirectToolSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolDisplayFields,
  getDirectToolVariantDisplayFields,
  getDirectToolSchemaRelations,
  getDirectToolSchemaVariants,
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
import { AGENT_TOOL_COMMANDS } from './agent-contract.js';

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

function scopeCompactSchema(
  fields: ReturnType<typeof formatToolFieldsJson>,
  variants: ReturnType<typeof getDirectToolSchemaVariants>,
  variantFields: ReturnType<typeof getDirectToolVariantDisplayFields>
) {
  if (
    variants.length < 2 ||
    variants.some(variant => !variant.fields || variant.fields.length === 0)
  ) {
    return { fields, variants };
  }

  const sharedNames = variants
    .slice(1)
    .reduce(
      (names, variant) =>
        new Set([...names].filter(name => variant.fields!.includes(name))),
      new Set(variants[0]!.fields)
    );
  const typedByVariant = new Map(
    variants.map(variant => [
      variant.name,
      (variantFields[variant.name]?.length
        ? variantFields[variant.name]
        : fields.filter(field =>
            variant.fields!.some(
              parent =>
                field.name === parent || field.name.startsWith(`${parent}.`)
            )
          )
      ).map(field => ({
        ...field,
        required: field.required || variant.requires.includes(field.name),
      })),
    ])
  );
  const common = new Set(
    [...sharedNames].filter(name => {
      const rendered = variants.map(variant =>
        typedByVariant.get(variant.name)?.find(field => field.name === name)
      );
      return (
        rendered.every(Boolean) &&
        new Set(rendered.map(field => formatCompactField(field!))).size === 1
      );
    })
  );
  const scoped = new Set(variants.flatMap(variant => variant.fields ?? []));
  const isScoped = (name: string): boolean =>
    [...scoped].some(
      parent => name === parent || name.startsWith(`${parent}.`)
    );

  return {
    fields: fields.filter(
      field => !isScoped(field.name) || common.has(field.name)
    ),
    variants: variants.map(variant => ({
      ...variant,
      fields: (typedByVariant.get(variant.name) ?? [])
        .filter(field => !common.has(field.name))
        .map(formatCompactField),
    })),
  };
}

function compactRunCommand(toolName: string): string {
  return `${formatToolExampleCommand(toolName)} --compact`;
}

export async function printToolCatalogJson(
  options: ToolCatalogJsonOptions = {}
): Promise<void> {
  const metadata = await getOptionalToolMetadata();
  const toolNames = TOOL_DEFINITIONS.map(tool => tool.name);

  if (!options.full) {
    const catalog = {
      kind: 'octocode.toolCatalog',
      version: 1,
      toolCount: toolNames.length,
      output:
        'results[].{index,status?,meta,data?}; tool payload and continuations are row-local under data',
      commands: {
        fullCatalog: 'tools --json --full',
        schema: AGENT_TOOL_COMMANDS.schema,
        fullSchema: AGENT_TOOL_COMMANDS.fullSchema,
        run: AGENT_TOOL_COMMANDS.run,
      },
      tools: toolNames.map(toolName => ({
        name: toolName,
        category: getDirectToolCategory(toolName),
        description: formatConciseToolDescription(toolName, metadata, 38),
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
      schema: AGENT_TOOL_COMMANDS.fullSchema,
      compactSchema: 'tools <name> --scheme --json --compact',
      humanSchema: 'tools <name> --scheme',
      runCompact: AGENT_TOOL_COMMANDS.run,
      runEnvelope: AGENT_TOOL_COMMANDS.runEnvelope,
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
    const compactSchema = scopeCompactSchema(
      fields,
      variants,
      getDirectToolVariantDisplayFields(tool.name)
    );
    return {
      kind: 'octocode.toolSchema.compact',
      version: 1,
      name: tool.name,
      category: getDirectToolCategory(tool.name),
      description: formatConciseToolDescription(tool.name, metadata, 160),
      availability: getToolAvailability(tool.name),
      fields: compactSchema.fields.map(formatCompactField),
      ...(relations.length > 0 ? { relations } : {}),
      ...(compactSchema.variants.length > 0
        ? { variants: compactSchema.variants }
        : {}),
      ...(guidance.length > 0 ? { guidance } : {}),
      commands: {
        full: `tools ${tool.name} --scheme --json`,
        run: compactRunCommand(tool.name),
      },
    };
  }

  const inputSchema = JSON.parse(formatDirectToolSchemaText(tool.name));
  const commandPatterns = buildDirectToolCommandPatterns(tool.name);
  const autoFilledFields = getDirectToolAutoFilledFields(tool.name);

  return {
    kind: 'octocode.toolSchema',
    version: 1,
    name: tool.name,
    category: getDirectToolCategory(tool.name),
    description: extractShortDescription(fullDescription),
    inputSchema,
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
