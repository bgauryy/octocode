// CLI and MCP discovery content, rendered once here so the native binary
// serves `tools/list`, `initialize`, `schema`, `catalog` field lists and
// `<tool> --help` without Node. Every schema view is a core Zod schema emitted
// once as JSON: `query` is the row MCP lists, and each
// `--select operation=X` that operation's row. Execution takes one canonical JSON envelope.
import {
  AGENT_TEXT,
  MCP_INSTRUCTION_PARTS,
} from '@octocodeai/octocode-core/mcp';
import {
  buildOutputPresentationSchemas,
  continuationMeanings,
  DIRECT_TOOL_DEFINITIONS,
  getPublicToolCatalog,
  operationRows,
  publishedInputSchema,
  publishedJson,
  publishedRow,
  publishedRowSchema,
  selectedRowSchemas,
  sentForms,
  rowVariants,
  toolVariants,
} from '@octocodeai/octocode-core/schema';
import { outputFields } from './output-view.ts';

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

/** How many rows one fresh call takes: the published envelope's `queries` bound. */
function queriesPerCall(definition: Definition): string {
  const { properties } = publishedInputSchema(definition);
  const queries =
    properties && typeof properties === 'object'
      ? (properties as Json).queries
      : undefined;
  const max =
    queries && typeof queries === 'object' && 'maxItems' in queries
      ? queries.maxItems
      : undefined;
  if (typeof max !== 'number')
    throw new Error(`${definition.name}: queries has no maxItems`);
  return max === 1 ? 'One query per call.' : `Up to ${max} queries per call.`;
}

const shellQuote = (text: string): string =>
  `'${text.replaceAll("'", `'\\''`)}'`;

/** What follows clap's help for `<tool> --help`: required fields and one runnable example (workspace-relative, from the variant's Zod `examples` meta). */
export function toolHelpSection(definition: Definition): string {
  const { name } = definition;
  const variants = toolVariants(definition);
  const lines: string[] = [];
  const jsonExamples = variants.flatMap(variant =>
    variant.example ? [variant.example] : []
  );
  lines.push('Examples:');
  for (const example of jsonExamples)
    lines.push(
      `  octocode ${name} ${shellQuote(exampleJson({ queries: [example] }))}`
    );
  lines.push('', AGENT_TEXT.helpInput, queriesPerCall(definition));
  const [only] = variants;
  if (only && variants.length === 1) {
    lines.push(
      '',
      `Required: ${only.requires.join(', ') || 'none; every field is optional.'}`
    );
  } else if (variants.length > 1) {
    lines.push('', 'Required fields, by operation:');
    // Each row names every operation it takes, so none is left unlisted.
    // Variants that differ only in their example (one per ecosystem) share
    // one row: the examples above show them. A form whose operations another
    // form also takes with strictly fewer required fields is a sub-form an
    // optional field selects (graphQuery deps with view): its row is labelled
    // "(with <field>)", so that field reads as required only when sent.
    const selectors = (variant: (typeof variants)[number]): string[] => {
      if (variant.operations.length === 0) return [];
      const covering = variants
        .filter(
          other =>
            other !== variant &&
            variant.operations.every(operation =>
              other.operations.includes(operation)
            ) &&
            other.requires.length < variant.requires.length &&
            other.requires.every(field => variant.requires.includes(field))
        )
        .sort((a, b) => b.requires.length - a.requires.length)[0];
      return covering
        ? variant.requires.filter(field => !covering.requires.includes(field))
        : [];
    };
    const forms = [
      ...new Map(
        variants.map(variant => {
          const selected = selectors(variant);
          const operations = variant.operations.join(', ') || variant.name;
          const label = selected.length
            ? `${operations} (with ${selected.join(', ')})`
            : operations;
          const requires = variant.requires.join(', ');
          return [`${label}\0${requires}`, { label, requires }] as const;
        })
      ).values(),
    ];
    const width = Math.max(...forms.map(form => form.label.length));
    for (const form of forms)
      lines.push(`  ${form.label.padEnd(width)}  ${form.requires}`.trimEnd());
    lines.push(
      '',
      `Each form's example: octocode schema ${name} --view variants`
    );
  }
  lines.push('', `Fields: octocode schema ${name}`);
  return lines.join('\n');
}

/**
 * The `catalog` row's compact field list: each row form's published fields,
 * required first (`*`) then optional (`?`), truncated with `…`; forms are
 * joined by ` | ` and led by the operation they take: `[operation:"match",
 * path*, rule*, …] | …`. The operation is a field of the row, never a wrapper.
 */
export function compactFields(definition: Definition): string {
  const shown = new Set(
    rowVariants(publishedRow(definition)).flatMap(row =>
      Object.keys(row.shape)
    )
  );
  const forms = toolVariants(definition);
  const max =
    forms.length === 1 ? CATALOG_FIELDS_SINGLE : CATALOG_FIELDS_PER_FORM;
  const variants = sentForms(definition);
  // The field whose values tell the forms apart: operation, else the one
  // whose values differ between forms (artifactSearch `ecosystem`).
  const valuesOf = (
    schema: Parameters<typeof publishedJson>[0] | undefined
  ): string[] => {
    const json = schema ? publishedJson(schema) : {};
    return Array.isArray(json.enum)
      ? json.enum.map(String)
      : typeof json.const === 'string'
        ? [json.const]
        : [];
  };
  const [first] = variants;
  const selector =
    !first || variants.length < 2
      ? undefined
      : (['operation', ...Object.keys(first.shape)].find(field => {
          const sets = variants.map(variant =>
            valuesOf(variant.shape[field]).join('|')
          );
          return sets.every(Boolean) && new Set(sets).size > 1;
        }) ??
        (variants.some(variant => 'operation' in variant.shape)
          ? 'operation'
          : undefined));
  return variants
    .map((variant, index) => {
      const requires = forms[index]?.requires ?? [];
      const fields = Object.entries(variant.shape)
        .filter(
          ([field, schema]) =>
            shown.has(field) &&
            field !== selector &&
            !('not' in publishedJson(schema))
        )
        .map(([field]) => ({ field, required: requires.includes(field) }))
        .toSorted(
          (left, right) =>
            Number(right.required) - Number(left.required) ||
            left.field.localeCompare(right.field)
        );
      const listed = fields
        .slice(0, max)
        .map(({ field, required }) => `${field}${required ? '*' : '?'}`);
      if (fields.length > max) listed.push('…');
      const values = selector ? valuesOf(variant.shape[selector]) : [];
      const optional = selector && requires.includes(selector) ? '' : '?';
      const lead = values.length
        ? [
            `${selector}${optional}:${values.map(value => JSON.stringify(value)).join('|')}`,
          ]
        : [];
      return `[${[...lead, ...listed].join(', ')}]`;
    })
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

/** A tool's MCP listing and CLI schema views share the exact public input contract. */
function rendered(definition: Definition) {
  const { name } = definition;
  const inputSchema = publishedInputSchema(definition);
  const row = publishedRowSchema(definition);
  return {
    views: {
      query: view(definition, row, { inputSchema }),
      variants: {
        name,
        description: definition.description,
        variants: toolVariants(definition),
      },
    },
    selections: Object.fromEntries(
      Object.entries(selectedRowSchemas(definition)).map(
        ([selected, schema]) => [selected, view(definition, schema)]
      )
    ),
    mcp: {
      title: definition.title,
      description: definition.description,
      ...(definition.annotations && { annotations: definition.annotations }),
      inputSchema,
    },
  };
}

/**
 * `fields`, the tool's output reference, with the meaning of each `next`
 * channel the tool can emit as `next.<channel>`, right after `next` itself:
 * the field an answer holds is one lookup away.
 */
function withNextChannels(
  name: string,
  fields: Record<string, string>
): Record<string, string> {
  const channels = Object.entries(continuationMeanings(name)).map(
    ([channel, meaning]) => [`next.${channel}`, meaning] as const
  );
  const out: Record<string, string> = {};
  for (const [path, text] of Object.entries(fields)) {
    out[path] = text;
    if (path === 'next')
      for (const [key, meaning] of channels) out[key] = meaning;
  }
  for (const [key, meaning] of channels) out[key] ??= meaning;
  return out;
}

/** The `cli-presentation.json` the native binary embeds beside the enforcement contract. */
export function buildCliPresentation(): Json {
  const catalog = getPublicToolCatalog();
  // The enforcement IR carries no prose; each presentation schema brings its own $defs.
  const outputSchemas = buildOutputPresentationSchemas();
  return {
    fingerprint: catalog.fingerprint,
    // MCP `initialize` and the CLI catalog serve these parts, composed for the enabled tools:
    // head, the routes whose `needs` are all enabled, body, then the enabled tools' blocks.
    instructions: MCP_INSTRUCTION_PARTS,
    // Core's other agent text: the CLI catalog's notes and the MCP resource descriptions.
    agentText: {
      cliNotes: AGENT_TEXT.cliNotes,
      resources: AGENT_TEXT.resources,
    },
    tools: DIRECT_TOOL_DEFINITIONS.map(definition => {
      const { name } = definition;
      const outputSchema = outputSchemas[name];
      return {
        name,
        title: definition.title,
        variantNames: Object.keys(operationRows(definition)),
        fields: compactFields(definition),
        help: toolHelpSection(definition),
        // `schema <tool> --view output`: one copy, whichever row form is served.
        output: outputSchema
          ? withNextChannels(name, outputFields(outputSchema, {}))
          : {},
        ...rendered(definition),
      };
    }),
  };
}
