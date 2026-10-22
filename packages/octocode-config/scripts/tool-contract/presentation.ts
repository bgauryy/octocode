// CLI discovery content, rendered once here so the native binary serves
// `schema`, `catalog` field lists and `<tool> --help` without Node. Every
// schema view is a core Zod schema emitted once as JSON: `query` is the row
// MCP lists, `full` the canonical row with unavailable addons omitted, and
// each `--select operation=X` that operation's row. `query` also lists the
// continuation fields the published row hides, one line each, and names the
// other accepted fields it hides (per operation, or the whole row's) with
// the `--select` or `--view full` call that shows their schemas.
import { buildCliInstructions } from '@octocodeai/octocode-core/mcp';
import { cliExamples, cliOutput, cliSpec, schemaFieldNames } from './cli-flags.ts';
import {
  CLASIFY_SOURCE_TOOL_NAMES,
  DIRECT_TOOL_DEFINITIONS,
  getPublicToolCatalog,
  getDirectToolDefinitionsWithAddons,
  getPublicToolCatalogWithAddons,
  continuationFields,
  getToolSchemaVariants,
  operationSchemas,
  publishedFormFields,
  publishedInputSchema,
  publishedJson,
} from '@octocodeai/octocode-core/schema';

type Json = Record<string, unknown>;
type Definition = (typeof DIRECT_TOOL_DEFINITIONS)[number];

/** Widest one-line help example; a longer one prints one field per line. */
const EXAMPLE_WIDTH = 100;
/** Fields a `catalog` row lists: more for a tool with one row form. */
const CATALOG_FIELDS_SINGLE = 8;
const CATALOG_FIELDS_PER_FORM = 6;

function exampleJson(example: Json): string {
  const line = JSON.stringify(example);
  if (line.length <= EXAMPLE_WIDTH) return line;
  const fields = Object.entries(example).map(
    ([key, value]) => `    ${JSON.stringify(key)}: ${JSON.stringify(value)}`
  );
  return `{\n${fields.join(',\n')}\n  }`;
}

const shellQuote = (text: string): string =>
  `'${text.replaceAll("'", `'\\''`)}'`;

/** What follows clap's help for `<tool> --help`: required fields and one runnable example (workspace-relative, from the variant's Zod `examples` meta). */
export function toolHelpSection(
  name: string,
  availableTools?: readonly string[]
): string {
  const variants = getToolSchemaVariants(name);
  const lines: string[] = [];
  const flagExamples = cliExamples(name);
  const jsonExamples = variants
    .map(variant => variant.example)
    .filter(
      (candidate): candidate is Json =>
        candidate !== undefined &&
        (!availableTools ||
          exampleUsesAvailableTools(candidate, availableTools))
    );
  lines.push('Examples:');
  for (const example of flagExamples) lines.push(`  ${example}`);
  // One JSON example beside the flags; a JSON-only tool shows every form's.
  for (const example of flagExamples.length > 0 ? jsonExamples.slice(0, 1) : jsonExamples)
    lines.push(`  octocode ${name} ${shellQuote(exampleJson(example))}`);
  const output = cliOutput(name);
  if (output) lines.push('', `Output: ${output}`);
  lines.push(
    '',
    flagExamples.length > 0
      ? 'JSON form: one query object, or {"queries":[…]} to batch; flags build the same query.'
      : 'Input: one JSON query object, or {"queries":[…]} to batch.'
  );
  if (variants.length === 1) {
    lines.push(
      '',
      `Required: ${variants[0]!.requires.join(', ') || 'none; every field is optional.'}`
    );
  } else if (variants.length > 1) {
    lines.push('', 'Required fields, by operation:');
    // Each row names every operation it takes, so none is left unlisted.
    const forms = variants.map(variant => ({
      ...variant,
      label: variant.operations.join(', ') || variant.name,
    }));
    const width = Math.max(...forms.map(form => form.label.length));
    for (const form of forms)
      lines.push(
        `  ${form.label.padEnd(width)}  ${form.requires.join(', ')}`.trimEnd()
      );
  }
  if (variants.length > 1)
    lines.push('', `Each form's example: octocode schema ${name} --view variants`);
  lines.push('', `Fields: octocode schema ${name} (complete: --view full)`);
  return lines.join('\n');
}

/** A clasify example's source names an exposed reader (a URL reads back through localFetch). */
function exampleUsesAvailableTools(
  example: Json,
  availableTools: readonly string[]
): boolean {
  const source = example.source as Json | undefined;
  if (!source) return true;
  const reader = typeof source.tool === 'string' ? source.tool : 'localFetch';
  return availableTools.includes(reader);
}

/** Every subset of clasify's source readers, as the key the CLI selects by. */
function sourceSubsets(): string[][] {
  return CLASIFY_SOURCE_TOOL_NAMES.reduce<string[][]>(
    (subsets, tool) => [
      ...subsets,
      ...subsets.map(subset => [...subset, tool]),
    ],
    [[]]
  );
}

/**
 * The `catalog` row's compact field list: the published fields of each row
 * form, required first (`*`) then optional (`?`), truncated with `…`. A tool
 * with several forms lists each as one flat row, alternatives joined by
 * ` | `, the fields that tell the forms apart first with the values they
 * take there: `[operation:"match", path*, rule*, …] | …`. The selector is a
 * field of the row, never a wrapper around it.
 */
export function compactFields(definition: Definition): string {
  const forms = publishedFormFields(definition.inputSchema);
  const list = (
    fields: readonly {
      name: string;
      required: boolean;
      values?: readonly string[];
    }[],
    max: number
  ): string => {
    const selectors = fields
      .filter(field => field.values)
      .map(
        field =>
          `${field.name}${field.required ? '' : '?'}:${field
            .values!.map(value => JSON.stringify(value))
            .join('|')}`
      );
    const rest = fields.filter(field => !field.values);
    const shown = rest
      .slice(0, max)
      .map(field => `${field.name}${field.required ? '*' : '?'}`);
    if (rest.length > max) shown.push('…');
    return `[${[...selectors, ...shown].join(', ')}]`;
  };
  return forms
    .map(fields =>
      list(
        fields,
        forms.length === 1 ? CATALOG_FIELDS_SINGLE : CATALOG_FIELDS_PER_FORM
      )
    )
    .join(' | ');
}

/** One `schema` view: the tool's description beside one emitted row schema. */
const view = (
  definition: Definition,
  querySchema: Json,
  extra: Json = {}
): Json => ({
  name: definition.name,
  description: definition.description,
  querySchema,
  ...extra,
});

const fieldNames = (schema: Json): string[] =>
  Object.keys((schema.properties as Json | undefined) ?? {});

/**
 * The accepted fields the `query` view's row leaves out beyond its
 * continuation fields and the request brief, by name only: per operation
 * with the `--select` call that shows them, or for a one-form tool with the
 * `--view full` call. Empty when the row shows every field.
 */
function hiddenFields(
  definition: Definition,
  row: Json,
  continuation: Json,
  canonical: Json
): Json {
  const { name, inputSchema } = definition;
  const shown = new Set([
    ...fieldNames(row),
    ...Object.keys(continuation),
    'mainGoal',
    'reasoning',
  ]);
  const hidden = (schema: Json): string[] =>
    fieldNames(schema).filter(field => !shown.has(field));
  const operations = Object.entries(operationSchemas(inputSchema));
  if (operations.length > 0) {
    const byOperation = Object.fromEntries(
      operations
        .map(
          ([operation, schema]) =>
            [operation, hidden(publishedJson(schema))] as const
        )
        .filter(([, fields]) => fields.length > 0)
    );
    return Object.keys(byOperation).length === 0
      ? {}
      : {
          operationFields: byOperation,
          select: `octocode schema ${name} --view query --select operation=<operation> shows that operation's fields`,
        };
  }
  const more = hidden(canonical);
  return more.length === 0
    ? {}
    : {
        moreFields: more,
      };
}

/**
 * The fields MCP publishes beside `queries` (the request envelope), as
 * schemas: a row view shows one query, so these are listed next to it.
 */
function envelopeFields(inputSchema: Definition['inputSchema']): Json {
  const { queries: _queries, ...fields } = (publishedInputSchema(inputSchema)
    .properties ?? {}) as Json;
  return Object.keys(fields).length === 0 ? {} : { envelopeFields: fields };
}

/** A tool's `schema` views and `--select operation=X` views. */
function rendered(
  definition: Definition,
  querySchema: Json
): { views: Json; selections: Json } {
  const { name, inputSchema } = definition;
  const variants = getToolSchemaVariants(name);
  // Core's presentation catalog owns any availability-dependent row changes.
  const { $schema: _schema, ...canonical } = querySchema;
  const row = publishedInputSchema(inputSchema, 'row');
  // The row hides the fields a `next.*` page or `hints.*` lead fills; name them once here.
  const continuation = continuationFields(inputSchema);
  const envelope = envelopeFields(inputSchema);
  return {
    views: {
      query: view(definition, row, {
        full: `octocode schema ${name} --view full shows every field`,
        ...envelope,
        continuationFields: continuation,
        ...hiddenFields(definition, row, continuation, canonical),
      }),
      full: view(definition, canonical, {
        ...envelope,
        usage: [`octocode ${name} '{"queries":[ … ]}'`],
      }),
      variants: { name, description: definition.description, variants },
    },
    selections: Object.fromEntries(
      Object.entries(operationSchemas(inputSchema)).map(
        ([operation, schema]) => [
          `operation=${operation}`,
          view(definition, publishedJson(schema)),
        ]
      )
    ),
  };
}

/** The `cli-presentation.json` the native binary embeds beside the enforcement contract. */
export function buildCliPresentation(): Json {
  const availableTools = DIRECT_TOOL_DEFINITIONS.map(tool => tool.name);
  const definitions = getDirectToolDefinitionsWithAddons({ availableTools });
  const rankedDefinitions = getDirectToolDefinitionsWithAddons({
    availableTools,
    selection: true,
  });
  const catalog = getPublicToolCatalogWithAddons({ availableTools });
  const rankedCatalog = getPublicToolCatalogWithAddons({
    availableTools,
    selection: true,
  });
  return {
    fingerprint: getPublicToolCatalog().fingerprint,
    instructions: {
      ordinary: buildCliInstructions(),
      selection: buildCliInstructions({ selection: true }),
      clasify: buildCliInstructions({ availableTools }),
      both: buildCliInstructions({
        availableTools,
        selection: true,
      }),
    },
    clasifySourceTools: [...CLASIFY_SOURCE_TOOL_NAMES],
    tools: definitions.map(definition => {
      const { name } = definition;
      const querySchema = catalog.tools.find(tool => tool.name === name)!
        .querySchema as Json;
      const rankedSchema = rankedCatalog.tools.find(tool => tool.name === name)!
        .querySchema as Json;
      const rankedDefinition = rankedDefinitions.find(
        tool => tool.name === name
      )!;
      return {
        name,
        shortDescription: definition.shortDescription,
        variantNames: Object.keys(operationSchemas(definition.inputSchema)),
        fields: compactFields(definition),
        help: toolHelpSection(name, name === 'clasify' ? [] : undefined),
        cli: cliSpec(
          name,
          querySchema,
          schemaFieldNames(publishedInputSchema(definition.inputSchema, 'row') as Json)
        ),
        ...rendered(definition, querySchema),
        // clasify's views for each exposed set of source readers, keyed by
        // the exposed readers in CLASIFY_SOURCE_TOOL_NAMES order.
        ...(name === 'clasify' && {
          sourceViews: Object.fromEntries(
            sourceSubsets().map(subset => [
              subset.join(','),
              rendered(
                getDirectToolDefinitionsWithAddons({
                  availableTools: subset,
                }).find(tool => tool.name === name)!,
                getPublicToolCatalogWithAddons({
                  availableTools: subset,
                }).tools.find(tool => tool.name === name)!.querySchema as Json
              ),
            ])
          ),
        }),
        // Emit only changed views; Rust selects them using native runtime truth.
        ...(JSON.stringify(querySchema) !== JSON.stringify(rankedSchema) && {
          selection: {
            fields: compactFields(rankedDefinition),
            // Flags for the selection fields (clasifyContext) the row adds.
            cli: cliSpec(
              name,
              rankedSchema,
              schemaFieldNames(publishedInputSchema(rankedDefinition.inputSchema, 'row') as Json)
            ),
            ...rendered(rankedDefinition, rankedSchema),
          },
        }),
      };
    }),
  };
}
