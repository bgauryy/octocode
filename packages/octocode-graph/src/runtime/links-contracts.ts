// Contract entities (F12 of the precise-graph RFC): names a repository declares in a data contract, which code in
// any language and its docs refer to by spelling. Glean, Kythe and JetBrains join languages the same way: through a
// language-neutral entity, keeping each language's own nodes. Three kinds are read, by precedence for a shared name:
// - a tool of a registry in the MCP tool-definition shape (a JSON `tools` array whose entries have a string `name`
//   and an object `inputSchema`): `<registry>#<tool>`;
// - an input field such a tool declares, its `inputSchema` properties at any depth with local `$ref`s followed:
//   `<registry>#field.<name>`, one per name however many tools share it;
// - a property of a JSON Schema document (a top-level `$schema` naming json-schema.org): `<file>#<name>`.
// A field or property counts only with a code-shaped name (`matchString`, `api_host`), never a word (`path`). A quoted
// string literal in a code file that spells a name exactly (`readResponse('lspSearch', …)` in TypeScript,
// `"matchString"` in Rust) references it; a doc mention resolves in links-docs.ts. Two entities of one precedence
// sharing a name are ambiguous and link nowhere. Files are read, never run; the literals are found by Octocode's own
// localSearch, every page walked.
import { readFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { answerBase, nativeWorkspaceRoot } from './lib.ts';
import { pagedTool } from './tools.ts';
import type { Link, UnresolvedLink } from './types.ts';

/** A name a contract declares, at the line that spells it. */
export interface ContractEntity {
  id: string;
  file: string;
  name: string;
  line: number;
  kind: 'tool' | 'field' | 'property';
}

const PRECEDENCE: ContractEntity['kind'][] = ['tool', 'field', 'property'];
const CODE_SHAPED = /[a-z][A-Z]|_/;

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

// The tools of a registry in the MCP tool-definition shape, or none.
function registryTools(
  json: unknown
): { name: string; inputSchema: unknown }[] {
  if (!isRecord(json) || !Array.isArray(json.tools)) return [];
  const tools = json.tools.filter(
    (t): t is { name: string; inputSchema: unknown } =>
      isRecord(t) && typeof t.name === 'string' && isRecord(t.inputSchema)
  );
  return tools.length === json.tools.length ? tools : [];
}

// Every `properties` key under `schema`, following `#/...` references into `doc` once each.
function propertyNames(
  schema: unknown,
  doc: unknown,
  out = new Set<string>(),
  seen = new Set<string>()
) {
  if (Array.isArray(schema)) {
    for (const item of schema) propertyNames(item, doc, out, seen);
    return out;
  }
  if (!isRecord(schema)) return out;
  if (
    typeof schema.$ref === 'string' &&
    schema.$ref.startsWith('#/') &&
    !seen.has(schema.$ref)
  ) {
    seen.add(schema.$ref);
    let target: unknown = doc;
    for (const part of schema.$ref.slice(2).split('/'))
      target = isRecord(target)
        ? target[part.replaceAll('~1', '/').replaceAll('~0', '~')]
        : undefined;
    propertyNames(target, doc, out, seen);
  }
  if (isRecord(schema.properties))
    for (const name of Object.keys(schema.properties)) out.add(name);
  for (const [key, value] of Object.entries(schema))
    if (key !== '$ref' && key !== '$defs' && key !== 'definitions')
      propertyNames(value, doc, out, seen);
  return out;
}

/** Every tool, tool input field and schema property the listing's contracts declare. */
export function contractEntities(
  root: string,
  listing: readonly string[]
): ContractEntity[] {
  const entities: ContractEntity[] = [];
  for (const file of listing) {
    if (!file.endsWith('.json')) continue;
    let text: string;
    try {
      text = readFileSync(join(root, file), 'utf8');
    } catch {
      continue;
    }
    const registry = text.includes('"inputSchema"');
    const schema = /"\$schema"\s*:\s*"https?:\/\/json-schema\.org/.test(text);
    if (!registry && !schema) continue;
    let json: unknown;
    try {
      json = JSON.parse(text);
    } catch {
      continue;
    }
    const lines = text.split('\n');
    const lineOf = (pattern: RegExp) => {
      const at = lines.findIndex(l => pattern.test(l));
      return at < 0 ? 1 : at + 1;
    };
    const key = (name: string) => new RegExp(`"${escape(name)}"\\s*:`);
    const tools = registry ? registryTools(json) : [];
    const fields = new Set<string>();
    for (const tool of tools) {
      entities.push({
        id: `${file}#${tool.name}`,
        file,
        name: tool.name,
        line: lineOf(new RegExp(`"name"\\s*:\\s*"${escape(tool.name)}"`)),
        kind: 'tool',
      });
      for (const name of propertyNames(tool.inputSchema, json))
        fields.add(name);
    }
    for (const name of [...fields].filter(n => CODE_SHAPED.test(n)).sort())
      entities.push({
        id: `${file}#field.${name}`,
        file,
        name,
        line: lineOf(key(name)),
        kind: 'field',
      });
    // A top-level `$schema` names a JSON Schema document (a nested one is a schema inside another file).
    if (
      !tools.length &&
      isRecord(json) &&
      typeof json.$schema === 'string' &&
      /json-schema\.org/.test(json.$schema)
    )
      for (const name of [...propertyNames(json, json)]
        .filter(n => CODE_SHAPED.test(n))
        .sort())
        entities.push({
          id: `${file}#${name}`,
          file,
          name,
          line: lineOf(key(name)),
          kind: 'property',
        });
  }
  return entities;
}

/** The entities a spelled name references: those of the highest-precedence kind that declares it. */
export function entitiesNamed(entities: readonly ContractEntity[]) {
  const byName = new Map<string, ContractEntity[]>();
  for (const [name, list] of Map.groupBy(entities, e => e.name)) {
    const kind = PRECEDENCE.find(k => list.some(e => e.kind === k));
    byName.set(
      name,
      list.filter(e => e.kind === kind)
    );
  }
  return byName;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const QUOTES = ["'", '"', '`'];
// localSearch takes at most this many patterns per query (its matchString limit).
const PATTERNS_PER_QUERY = 100;

interface SearchFile {
  path: string;
  matches?: { line?: number }[];
}

/**
 * The code files' literals that spell a contract name: one `contract` link per citing line and entity. `code` is the
 * graph's own code files (root-relative); docs and the registries themselves are not code. An ambiguous name (two
 * registries declare it) is unresolved, with both candidates.
 */
export async function contractLinks(
  root: string,
  code: ReadonlySet<string>,
  entities: readonly ContractEntity[]
): Promise<{ links: Link[]; unresolved: UnresolvedLink[] }> {
  const links: Link[] = [];
  const unresolved: UnresolvedLink[] = [];
  if (!entities.length || !code.size) return { links, unresolved };
  const byName = entitiesNamed(entities);
  const patterns = [...byName.keys()].flatMap(name =>
    QUOTES.map(q => `${q}${name}${q}`)
  );
  const sites = new Map<string, Set<number>>();
  const workspace = await nativeWorkspaceRoot();
  for (let i = 0; i < patterns.length; i += PATTERNS_PER_QUERY) {
    const pages = await pagedTool('localSearch', {
      queries: [
        {
          path: root,
          matchString: patterns.slice(i, i + PATTERNS_PER_QUERY),
          caseMode: 'sensitive',
        },
      ],
    });
    for (const page of pages)
      for (const row of page.results ?? [])
        for (const f of (row.data as { files?: SearchFile[] } | undefined)
          ?.files ?? []) {
          const file = relative(
            root,
            resolve(answerBase(row.data, workspace), f.path)
          );
          if (!code.has(file)) continue;
          const lines = sites.get(file) ?? new Set<number>();
          for (const m of f.matches ?? [])
            if (typeof m.line === 'number') lines.add(m.line);
          sites.set(file, lines);
        }
  }
  for (const [file, lines] of sites) {
    let text: string[];
    try {
      text = readFileSync(join(root, file), 'utf8').split('\n');
    } catch {
      continue;
    }
    for (const line of [...lines].toSorted((a, b) => a - b)) {
      const source = text[line - 1] ?? '';
      for (const [name, declared] of byName) {
        if (!QUOTES.some(q => source.includes(`${q}${name}${q}`))) continue;
        if (declared.length === 1)
          links.push({
            source: file,
            line,
            target: declared[0].id,
            relation: 'references',
            rule: 'contract',
            check: 'ok',
          });
        else
          unresolved.push({
            source: file,
            line,
            text: name,
            rule: 'contract',
            reason: 'ambiguous',
            candidates: declared.map(e => e.id),
          });
      }
    }
  }
  return { links, unresolved };
}
