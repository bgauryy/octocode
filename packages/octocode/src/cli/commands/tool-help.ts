// `<tool> --help`: the binary's command help, then what the query needs —
// required fields and one runnable example — from the same public catalog
// `schema` serves. The binary embeds enforcement only; examples are
// presentation, which this launcher owns.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { SchemaJsonObject } from '@octocodeai/config/schema';
import { nativeCommand } from '../native-delegate.js';

const execFileAsync = promisify(execFile);

/** Widest one-line example; a longer one prints as indented JSON. */
const EXAMPLE_WIDTH = 100;

/** Catalog examples root absolute paths here; help shows them relative. */
const EXAMPLE_ROOT = '/ABS/repo/';

function relativeExample(value: unknown): unknown {
  if (typeof value === 'string')
    return value.startsWith(EXAMPLE_ROOT)
      ? value.slice(EXAMPLE_ROOT.length)
      : value;
  if (Array.isArray(value)) return value.map(relativeExample);
  if (value && typeof value === 'object')
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [key, relativeExample(child)])
    );
  return value;
}

/** One line; past the width, one top-level field per line (still one shell argument). */
function exampleJson(example: SchemaJsonObject): string {
  const line = JSON.stringify(example);
  if (line.length <= EXAMPLE_WIDTH) return line;
  const fields = Object.entries(example).map(
    ([key, value]) => `    ${JSON.stringify(key)}: ${JSON.stringify(value)}`
  );
  return `{\n${fields.join(',\n')}\n  }`;
}

function shellQuote(text: string): string {
  return `'${text.replaceAll("'", `'\\''`)}'`;
}

function isObject(value: unknown): value is SchemaJsonObject {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

/** Each usage form's selectors (`field=value`) and `<field>`s, deduplicated. */
function requiredForms(forms: readonly string[]): string[][] {
  const seen = new Set<string>();
  const result: string[][] = [];
  for (const form of forms) {
    const fields = [...form.matchAll(/(\w+=\S+)|<([^>]+)>/g)].map(
      match => match[1] ?? match[2]!
    );
    const key = fields.join(',');
    if (fields.length === 0 || seen.has(key)) continue;
    seen.add(key);
    result.push(fields);
  }
  return result;
}

/** The help section that follows the binary's help for `tool`. */
export function toolHelpSection(
  tool: SchemaJsonObject,
  usageForms: readonly string[]
): string {
  const name = String(tool.name);
  const variants = (Array.isArray(tool.variants) ? tool.variants : []).filter(
    isObject
  );
  const lines = ['Input: one query object, or {"queries":[…]} to batch.'];
  if (variants.length === 1) {
    const requires = variants[0]!.requires;
    lines.push(
      '',
      `Required: ${(Array.isArray(requires) ? requires : []).join(', ')}`
    );
  } else if (variants.length > 1) {
    lines.push('', 'Required fields, by variant:');
    const width = Math.max(...variants.map(v => String(v.name).length));
    const requires = variants.map(v =>
      (Array.isArray(v.requires) ? v.requires : []).join(', ')
    );
    const requiresWidth = Math.max(...requires.map(r => r.length));
    variants.forEach((variant, index) => {
      const when = typeof variant.when === 'string' ? variant.when : '';
      lines.push(
        `  ${String(variant.name).padEnd(width)}  ${requires[index]!.padEnd(requiresWidth)}  ${when}`.trimEnd()
      );
    });
  } else {
    const forms = requiredForms(usageForms);
    if (forms.length === 0) {
      lines.push('', 'Required: none; every field is optional.');
    } else if (forms.length === 1) {
      lines.push('', `Required: ${forms[0]!.join(', ')}`);
    } else if (forms.length > 1) {
      lines.push('', 'Required (one form):');
      for (const fields of forms) lines.push(`  ${fields.join(', ')}`);
    }
  }
  const example =
    variants.find(variant => isObject(variant.example))?.example ??
    (Array.isArray(tool.examples) ? tool.examples.find(isObject) : undefined);
  if (isObject(example)) {
    lines.push(
      '',
      'Example:',
      `  octocode ${name} ${shellQuote(exampleJson(relativeExample(example) as SchemaJsonObject))}`
    );
  }
  lines.push('', `All fields: octocode schema ${name} --view query`);
  return lines.join('\n');
}

/**
 * Print the binary's help for `toolName`, then its input section. Returns
 * undefined when `toolName` is not a catalog tool, so the caller delegates.
 */
export async function runToolHelp(
  bin: string,
  toolName: string
): Promise<number | undefined> {
  const { getPublicToolCatalogWithAddons, schemaUsageForms } =
    await import('@octocodeai/config/schema');
  const tool = (
    getPublicToolCatalogWithAddons().tools as readonly SchemaJsonObject[]
  ).find(candidate => candidate.name === toolName);
  if (!tool) return undefined;
  const [command, args] = nativeCommand(bin, [toolName, '--help']);
  const { stdout } = await execFileAsync(command, args, {
    maxBuffer: 1024 * 1024,
  });
  process.stdout.write(
    `${stdout.trimEnd()}\n\n${toolHelpSection(tool, schemaUsageForms(tool.querySchema))}\n`
  );
  return 0;
}
