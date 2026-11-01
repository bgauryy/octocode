// CLI and MCP discovery content, rendered once here so the native binary
// serves `tools/list`, `initialize`, `schema`, `catalog` field lists and
// `<tool> --help` without Node. Every schema view is a core Zod schema emitted
// once as JSON: `query` is the row MCP lists, and each
// `--select operation=X` that operation's row. Execution takes one canonical JSON envelope.
import { MCP_INSTRUCTIONS } from '@octocodeai/octocode-core/mcp';
import {
  DIRECT_TOOL_DEFINITIONS,
  getPublicToolCatalog,
  operationRows,
  publishedInputSchema,
  publishedJson,
  publishedRow,
  publishedRowSchema,
  sentForms,
  rowVariants,
  toolVariants,
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
export function toolHelpSection(definition: Definition): string {
  const { name } = definition;
  const variants = toolVariants(definition);
  const lines: string[] = [];
  const jsonExamples = variants.flatMap(variant =>
    variant.example ? [variant.example] : []
  );
  lines.push('Examples:');
  for (const example of jsonExamples)
    lines.push(`  octocode ${name} ${shellQuote(exampleJson({ queries: [example] }))}`);
  lines.push('', 'Input: {"queries":[…]} for fresh calls; copy {"cursor":"…"} from a continuation.');
  const [only] = variants;
  if (only && variants.length === 1) {
    lines.push(
      '',
      `Required: ${only.requires.join(', ') || 'none; every field is optional.'}`
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
    lines.push('', `Each form's example: octocode schema ${name} --view variants`);
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
export function compactFields(definition: Definition, selection: boolean): string {
  const shown = new Set(rowVariants(publishedRow(definition, selection)).flatMap(row => Object.keys(row.shape)));
  const forms = toolVariants(definition);
  const max = forms.length === 1 ? CATALOG_FIELDS_SINGLE : CATALOG_FIELDS_PER_FORM;
  const variants = sentForms(definition);
  // The field whose values tell the forms apart: operation, else the one
  // whose values differ between forms (artifactSearch `ecosystem`).
  const valuesOf = (schema: Parameters<typeof publishedJson>[0] | undefined): string[] => {
    const json = schema ? publishedJson(schema) : {};
    return Array.isArray(json.enum) ? json.enum.map(String) : typeof json.const === 'string' ? [json.const] : [];
  };
  const [first] = variants;
  const selector = !first || variants.length < 2
    ? undefined
    : (['operation', ...Object.keys(first.shape)].find(field => {
        const sets = variants.map(variant => valuesOf(variant.shape[field]).join('|'));
        return sets.every(Boolean) && new Set(sets).size > 1;
      }) ?? (variants.some(variant => 'operation' in variant.shape) ? 'operation' : undefined));
  return variants
    .map((variant, index) => {
      const requires = forms[index]?.requires ?? [];
      const fields = Object.entries(variant.shape)
        .filter(([field, schema]) => shown.has(field) && field !== selector && !('not' in publishedJson(schema)))
        .map(([field]) => ({ field, required: requires.includes(field) }))
        .toSorted(
          (left, right) =>
            Number(right.required) - Number(left.required) || left.field.localeCompare(right.field)
        );
      const listed = fields.slice(0, max).map(({ field, required }) => `${field}${required ? '*' : '?'}`);
      if (fields.length > max) listed.push('…');
      const values = selector ? valuesOf(variant.shape[selector]) : [];
      const optional = selector && requires.includes(selector) ? '' : '?';
      const lead = values.length
        ? [`${selector}${optional}:${values.map(value => JSON.stringify(value)).join('|')}`]
        : [];
      return `[${[...lead, ...listed].join(', ')}]`;
    })
    .join(' | ');
}

/** One `schema` view: the tool's description beside one emitted row schema. */
const view = (definition: Definition, querySchema: Json, extra: Json = {}): Json => ({
  name: definition.name,
  description: definition.description,
  querySchema,
  ...extra,
});

/** A tool's MCP listing and CLI schema views share the exact public input contract. */
function rendered(definition: Definition, selection: boolean) {
  const { name } = definition;
  const inputSchema = publishedInputSchema(definition, selection);
  const row = publishedRowSchema(definition, selection);
  return {
    views: {
      query: view(definition, row, { inputSchema }),
      variants: { name, description: definition.description, variants: toolVariants(definition) },
    },
    selections: Object.fromEntries(
      Object.entries(operationRows(definition)).map(([operation, operationRow]) => [
        `operation=${operation}`,
        view(definition, publishedJson(operationRow)),
      ])
    ),
    mcp: {
      title: definition.title,
      description: definition.description,
      ...(definition.annotations && { annotations: definition.annotations }),
      inputSchema,
    },
  };
}

/** The `cli-presentation.json` the native binary embeds beside the enforcement contract. */
export function buildCliPresentation(): Json {
  const catalog = getPublicToolCatalog();
  return {
    fingerprint: catalog.fingerprint,
    // MCP `initialize` and the CLI catalog serve this one string.
    instructions: MCP_INSTRUCTIONS,
    tools: DIRECT_TOOL_DEFINITIONS.map(definition => {
      const { name } = definition;
      const plain = rendered(definition, false);
      const selected = rendered(definition, true);
      return {
        name,
        title: definition.title,
        variantNames: Object.keys(operationRows(definition)),
        fields: compactFields(definition, false),
        help: toolHelpSection(definition),
        ...plain,
        // Emit the selection variant only when it changes the row; Rust picks
        // it with the runtime's resolved switch.
        ...(JSON.stringify(plain.mcp) !== JSON.stringify(selected.mcp) && {
          selection: {
            fields: compactFields(definition, true),
            ...selected,
          },
        }),
      };
    }),
  };
}
