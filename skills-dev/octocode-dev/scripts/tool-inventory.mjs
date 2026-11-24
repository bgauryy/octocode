#!/usr/bin/env node
// Per-tool contract ↔ native implementation inventory for the octocode monorepo.
// Usage: node tool-inventory.mjs [toolName ...] [--json] [--root <repo>]
//        node tool-inventory.mjs [toolName ...] --schema-diff <old contract json> [--root <repo>]
//        node tool-inventory.mjs [toolName ...] --sizes [--root <repo>]
// Reads the generated native contract + field-effect coverage, then counts how often
// each input field name (camelCase and snake_case) appears in that tool's evidence files.
// A path with no coverage entry inherits its nearest declared ancestor's effects
// (`ranges[].startLine` -> `ranges[]` -> `ranges`), else the `common` entry for its
// top-level name. Paths are spelled as in tests/field_effect_coverage.rs, which gates them.
// Zero-hit fields are CANDIDATES for unused/misaligned inputs — confirm by reading code.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { diffCaptures, sizeCapture } from '../../../skills/octocode-agentic/scripts/mcp-surface.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Usage: node tool-inventory.mjs [toolName ...] [--json] [--root <repo>]
       node tool-inventory.mjs [toolName ...] --schema-diff <old contract json> [--root <repo>]
       node tool-inventory.mjs [toolName ...] --sizes [--root <repo>]

Default: counts, per tool, how often each contract input field appears in that tool's native module and
field-effect evidence files. Prints a table, or JSON with --json. A zero-hit field is a candidate for an unused or
misaligned input: confirm by reading code. The module is crates/*/src/<snake_name> (folder and .rs root), else the
src of the one crate whose Cargo.toml description names the tool; "none found" when neither exists.

--schema-diff <file>  structural diff of the published schemas against an older generated file, ignoring prose
                      (description, title, $comment): an old cli-presentation.json compares each tool's MCP
                      definition (inputSchema, annotations); an old tool-contract.json compares inputSchema and
                      outputSchema. $refs are inlined first, so renumbered $defs are no change. Get the old file
                      with \`git show <rev>:packages/octocode-config/contract/cli-presentation.json > old.json\`.
                      Prints {ok, kind, before, after, structural, counts, changes[], prose}; changes carry
                      {tool, path, kind, change, before, after}.
--sizes               UTF-8 bytes of what each tool tells an agent, from cli-presentation.json: descriptionBytes
                      (MCP tool description), schemaProseBytes (description/title text in its MCP inputSchema),
                      proseBytes (their sum), bytes (the compact MCP definition), cliHelpBytes (CLI --help), and its
                      family (siblings), familyMedianProseBytes and ratioToFamilyMedian.

Tool names limit every mode to those tools. Read-only.
Errors print {ok:false,error:{code,message}}. Exit codes: 0 done (--schema-diff: no structural change);
1 --schema-diff found a structural change, or the repository or contract cannot be read; 2 invalid arguments.`;

export const snake = s => s.replace(/[A-Z]/g, c => `_${c.toLowerCase()}`);
const isObject = value => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const bytes = text => (typeof text === 'string' ? Buffer.byteLength(text, 'utf8') : 0);

class UsageError extends Error {
  constructor(code, message, exitCode) {
    super(message);
    this.code = code;
    this.exitCode = exitCode;
  }
}

function findRoot(start) {
  let dir = resolve(start);
  while (true) {
    if (existsSync(join(dir, 'packages/octocode-native/crates/runtime'))) return dir;
    const up = dirname(dir);
    if (up === dir) throw new UsageError('rootNotFound', 'octocode monorepo root not found; pass --root <repo>', 1);
    dir = up;
  }
}

const readJson = file => {
  try {
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch (error) {
    throw new UsageError('unreadableFile', `${file}: ${error.message}`, 1);
  }
};

function rsFiles(path) {
  if (!existsSync(path)) return [];
  if (statSync(path).isFile()) return path.endsWith('.rs') ? [path] : [];
  return readdirSync(path).flatMap(name => rsFiles(join(path, name)));
}

/**
 * The native module of `tool` under `crates`: `<crate>/src/<snake>` (its folder and its `.rs` root, either may be
 * alone), else the `src` of the one crate whose manifest description names the tool (graphBuild and graphQuery
 * share tool-graph's). `{path, files}` with absolute paths, or null.
 */
export function moduleOf(crates, tool) {
  const dirs = readdirSync(crates).filter(dir => existsSync(join(crates, dir, 'Cargo.toml'))).toSorted();
  for (const dir of dirs) {
    const base = join(crates, dir, 'src', snake(tool));
    const files = [base, `${base}.rs`].filter(existsSync);
    if (files.length) return { path: existsSync(base) ? base : `${base}.rs`, files };
  }
  const names = new RegExp(`\\b${tool}\\b`);
  const named = dirs.filter(dir => names.test(/^description = "(.*)"$/m.exec(readFileSync(join(crates, dir, 'Cargo.toml'), 'utf8'))?.[1] ?? ''));
  return named.length === 1 ? { path: join(crates, named[0], 'src'), files: [join(crates, named[0], 'src')] } : null;
}

/** Field inventory per tool (the default mode). */
export function inventory(root, wanted = []) {
  const runtime = join(root, 'packages/octocode-native/crates/runtime');
  const crates = join(runtime, '..');
  const contract = readJson(join(root, 'packages/octocode-config/contract/tool-contract.json'));
  // The coverage file lives in octocode-runtime-base; its evidence paths are relative to crates/.
  const coverage = readJson(join(crates, 'runtime-base/src/contracts/field-effect-coverage.json'));

  /** `schema` with its `$ref` chain followed through the contract's `$defs`. */
  function deref(schema, seen = new Set()) {
    if (!schema?.$ref || seen.has(schema.$ref)) return schema;
    const parts = schema.$ref.slice(2).split('/').map(key => key.replace(/~1/g, '/').replace(/~0/g, '~'));
    return deref(parts.reduce((value, key) => value?.[key], contract), new Set([...seen, schema.$ref]));
  }

  /** Published property paths: `.name`, `[]` for array items, `{}` for additionalProperties; `x-cursor` state is private. */
  function leafFields(schema, prefix = '', out = new Map(), active = new Set()) {
    schema = deref(schema);
    if (!schema || active.has(schema)) return out;
    active = new Set([...active, schema]);
    for (const [name, child] of Object.entries(schema.properties ?? {})) {
      if (deref(child)?.['x-cursor']) continue;
      const path = prefix ? `${prefix}.${name}` : name;
      const prev = out.get(path);
      if (!prev || (!prev.description && child.description)) out.set(path, { description: child.description ?? '' });
      leafFields(child, path, out, active);
    }
    if (schema.items) leafFields(schema.items, `${prefix}[]`, out, active);
    if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
      leafFields(schema.additionalProperties, `${prefix}{}`, out, active);
    }
    for (const branch of branchesOf(schema)) leafFields(branch, prefix, out, active);
    return out;
  }

  /** `path` without its last segment (`.name`, `[]` or `{}`); empty at the top. */
  const parentPath = path => {
    const up = path.replace(/(\.[^.[\]{}]+|\[\]|\{\})$/, '');
    return up === path ? '' : up;
  };

  /** Declared effects for `path`: its own entry, the nearest ancestor's, else `common`; undefined when none. */
  function effectsOf(fields, path) {
    for (let at = path; at; at = parentPath(at)) if (at in fields) return fields[at];
    return coverage.common?.fields?.[path.split(/[.[{]/)[0]];
  }

  /** A schema's anyOf/oneOf/allOf branches. */
  const branchesOf = schema => ['anyOf', 'oneOf', 'allOf'].flatMap(key => schema?.[key] ?? []);

  const report = [];
  for (const tool of contract.tools) {
    if (wanted.length && !wanted.includes(tool.name)) continue;
    const cov = coverage.tools[tool.name] ?? { evidence: [], fields: {} };
    const module = moduleOf(crates, tool.name);
    const files = [...new Set([...(module?.files ?? []).flatMap(rsFiles), ...cov.evidence.map(e => join(crates, e)).flatMap(rsFiles)])];
    const corpus = files.map(f => readFileSync(f, 'utf8')).join('\n');
    const fields = [...leafFields(tool.querySchema)].map(([path, meta]) => {
      const leaf = path.split('.').pop().replace(/\[\]$/, '');
      const pattern = new RegExp(`\\b(${leaf}|${snake(leaf)})\\b`, 'g');
      return {
        path,
        classes: effectsOf(cov.fields, path) ?? [],
        hits: (corpus.match(pattern) ?? []).length,
        descriptionChars: meta.description.length,
      };
    });
    report.push({
      tool: tool.name,
      title: tool.title,
      variants: tool.variants.map(v => v.name),
      moduleDir: module ? relative(root, module.path) : null,
      evidenceFiles: files.map(f => relative(root, f)),
      fields,
      zeroHitFields: fields.filter(f => f.hits === 0).map(f => f.path),
      unclassifiedFields: fields.filter(f => effectsOf(cov.fields, f.path) === undefined).map(f => f.path),
      undescribedFields: fields.filter(f => f.descriptionChars === 0).map(f => f.path),
    });
  }
  return { fingerprint: contract.fingerprint, tools: report };
}

/** `schema` with every local `$ref` replaced by its target (looked up in `root`); a cycle keeps `{$recursive: n}`,
 * n refs back up the chain, so def names never decide equality. */
export function inlineRefs(schema, root, chain = []) {
  if (Array.isArray(schema)) return schema.map(item => inlineRefs(item, root, chain));
  if (!isObject(schema)) return schema;
  if (typeof schema.$ref === 'string' && schema.$ref.startsWith('#/')) {
    const back = chain.indexOf(schema.$ref);
    if (back >= 0) return { $recursive: chain.length - back };
    const target = schema.$ref.slice(2).split('/').map(key => key.replaceAll('~1', '/').replaceAll('~0', '~')).reduce((value, key) => value?.[key], root);
    if (target === undefined) throw new UsageError('unresolvedRef', `${schema.$ref} names nothing`, 1);
    const { $ref: _ref, ...rest } = schema;
    const resolved = inlineRefs(target, root, [...chain, schema.$ref]);
    const siblings = inlineRefs(rest, root, chain);
    // Keywords beside a $ref (a description, a default) join its target; a keyword both state stays a conjunction.
    if (!isObject(resolved) || Object.keys(siblings).some(key => Object.hasOwn(resolved, key))) return { ...siblings, allOf: [resolved] };
    return { ...resolved, ...siblings };
  }
  const out = {};
  for (const [key, value] of Object.entries(schema)) if (key !== '$defs') out[key] = inlineRefs(value, root, chain);
  return out;
}

/** A generated contract file as the capture shape mcp-surface.mjs diffs: `{kind, capture, tools}`. */
export function surfaceOf(json, file = 'contract') {
  if (Array.isArray(json?.tools) && json.tools.length && json.tools.every(tool => isObject(tool?.mcp))) {
    const tools = json.tools.map(tool => ({ name: tool.name, ...tool.mcp, inputSchema: inlineRefs(tool.mcp.inputSchema, tool.mcp.inputSchema) }));
    return { kind: 'cli-presentation', capture: { initialize: { instructions: JSON.stringify(json.instructions ?? null) } }, tools };
  }
  if (Array.isArray(json?.tools) && isObject(json.$defs)) {
    const tools = json.tools.map(tool => ({ name: tool.name, title: tool.title, inputSchema: inlineRefs(tool.inputSchema, json), outputSchema: inlineRefs(tool.outputSchema, json) }));
    return { kind: 'tool-contract', capture: { initialize: {} }, tools };
  }
  throw new UsageError('invalidContract', `${file} is neither a cli-presentation.json (tools[].mcp) nor a tool-contract.json (tools[] with $defs)`, 2);
}

const CURRENT = { 'cli-presentation': 'cli-presentation.json', 'tool-contract': 'tool-contract.json' };

/** Structural diff of `oldFile` against the current file of the same kind. */
export function schemaDiff(root, oldFile, wanted = []) {
  const before = surfaceOf(readJson(oldFile), oldFile);
  const currentFile = join(root, 'packages/octocode-config/contract', CURRENT[before.kind]);
  const after = surfaceOf(readJson(currentFile), currentFile);
  const only = side => (wanted.length ? { ...side, tools: side.tools.filter(tool => wanted.includes(tool.name)) } : side);
  return { kind: before.kind, before: oldFile, after: relative(root, currentFile), ...diffCaptures(only(before), only(after)) };
}

const median = values => {
  const sorted = values.toSorted((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};

/** Description bytes per tool beside its family's median, from parsed cli-presentation.json and tool-contract.json. */
export function sizes(presentation, contract, wanted = []) {
  const family = new Map(contract.tools.map(tool => [tool.name, tool.family]));
  const help = new Map(presentation.tools.map(tool => [tool.name, tool.help]));
  const tools = presentation.tools.map(tool => ({ name: tool.name, ...tool.mcp }));
  const rows = sizeCapture({ capture: { initialize: {}, toolsList: [{ result: { tools } }] }, tools }).tools.map(row => ({
    tool: row.name,
    family: family.get(row.name) ?? null,
    descriptionBytes: row.descriptionBytes,
    schemaProseBytes: row.schemaProseBytes,
    proseBytes: row.descriptionBytes + row.schemaProseBytes,
    bytes: row.bytes,
    cliHelpBytes: bytes(help.get(row.name)),
  }));
  const out = rows.map(row => {
    const siblings = rows.filter(other => other.family === row.family);
    const familyMedianProseBytes = median(siblings.map(other => other.proseBytes));
    return {
      ...row,
      siblings: siblings.filter(other => other !== row).map(other => other.tool),
      familyMedianProseBytes,
      ratioToFamilyMedian: familyMedianProseBytes ? Number((row.proseBytes / familyMedianProseBytes).toFixed(3)) : null,
    };
  });
  return { ok: true, unit: 'UTF-8 bytes', tools: wanted.length ? out.filter(row => wanted.includes(row.tool)) : out };
}

function parse(argv) {
  const options = { wanted: [], json: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--root' || arg === '--schema-diff') {
      if (!argv[i + 1] || argv[i + 1].startsWith('--')) throw new UsageError('invalidArguments', `${arg} needs a path`, 2);
      options[arg === '--root' ? 'root' : 'schemaDiff'] = resolve(argv[++i]);
    } else if (arg === '--json') options.json = true;
    else if (arg === '--sizes') options.sizes = true;
    else if (arg.startsWith('--')) throw new UsageError('invalidArguments', `unknown option ${arg} (see --help)`, 2);
    else options.wanted.push(arg);
  }
  if (options.sizes && options.schemaDiff) throw new UsageError('invalidArguments', '--sizes and --schema-diff are separate runs', 2);
  return options;
}

function printTable(report) {
  for (const t of report.tools) {
    console.log(`\n## ${t.tool} — ${t.title}`);
    console.log(`module: ${t.moduleDir ?? 'none found'}  files: ${t.evidenceFiles.length}  fields: ${t.fields.length}  variants: ${t.variants.join(', ') || '-'}`);
    console.log(`zero-hit (candidate unused/misnamed): ${t.zeroHitFields.join(', ') || 'none'}`);
    console.log(`unclassified in field-effect-coverage: ${t.unclassifiedFields.join(', ') || 'none'}`);
    console.log(`no field description (may be carried by a parent/branch): ${t.undescribedFields.length ? t.undescribedFields.join(', ') : 'none'}`);
  }
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    return 0;
  }
  try {
    const options = parse(argv);
    const root = options.root ?? findRoot(process.cwd());
    const contractDir = join(root, 'packages/octocode-config/contract');
    if (options.schemaDiff) {
      const result = schemaDiff(root, options.schemaDiff, options.wanted);
      const shown = relative(process.cwd(), result.before);
      console.log(JSON.stringify({ ...result, before: shown.startsWith('..') ? result.before : shown }, null, 1));
      return result.structural ? 1 : 0;
    }
    if (options.sizes) {
      console.log(JSON.stringify(sizes(readJson(join(contractDir, 'cli-presentation.json')), readJson(join(contractDir, 'tool-contract.json')), options.wanted), null, 1));
      return 0;
    }
    const report = inventory(root, options.wanted);
    if (options.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else printTable(report);
    return 0;
  } catch (error) {
    if (!(error instanceof UsageError)) throw error;
    console.log(JSON.stringify({ ok: false, error: { code: error.code, message: error.message } }));
    return error.exitCode;
  }
}

// exitCode, not exit(): a piped stdout flushes before the process ends.
if (isMainModule(import.meta.url)) process.exitCode = main(process.argv.slice(2));
