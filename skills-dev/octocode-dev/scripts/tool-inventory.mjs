#!/usr/bin/env node
// Per-tool contract ↔ native implementation inventory for the octocode monorepo.
// Usage: node tool-inventory.mjs [toolName ...] [--json] [--root <repo>]
// Reads the generated native contract + field-effect coverage, then counts how often
// each input field name (camelCase and snake_case) appears in that tool's evidence files.
// Zero-hit fields are CANDIDATES for unused/misaligned inputs — confirm by reading code.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

const args = process.argv.slice(2);
const json = args.includes('--json');
const rootIdx = args.indexOf('--root');
const wanted = args.filter((a, i) => !a.startsWith('--') && args[i - 1] !== '--root');

function findRoot(start) {
  let dir = resolve(start);
  while (true) {
    if (existsSync(join(dir, 'packages/octocode-native/crates/runtime'))) return dir;
    const up = dirname(dir);
    if (up === dir) throw new Error('octocode monorepo root not found; pass --root <repo>');
    dir = up;
  }
}

const root = rootIdx >= 0 ? resolve(args[rootIdx + 1]) : findRoot(process.cwd());
const runtime = join(root, 'packages/octocode-native/crates/runtime');
const contract = JSON.parse(readFileSync(join(root, 'packages/octocode-config/contract/tool-contract.json'), 'utf8'));
const coverage = JSON.parse(readFileSync(join(runtime, 'src/contracts/field-effect-coverage.json'), 'utf8'));

// Native module dirs whose names do not follow camel→snake of the tool name.
const DIR_OVERRIDES = {
  astTopology: 'ast_graph',
  ghSearchRepo: 'gh_search',
  ghSearchCode: 'gh_search',
  ghStructure: 'gh_search',
};
const snake = s => s.replace(/[A-Z]/g, c => `_${c.toLowerCase()}`);

function rsFiles(path) {
  if (!existsSync(path)) return [];
  if (statSync(path).isFile()) return path.endsWith('.rs') ? [path] : [];
  return readdirSync(path).flatMap(name => rsFiles(join(path, name)));
}

function leafFields(schema, prefix = '', out = new Map()) {
  for (const [name, child] of Object.entries(schema?.properties ?? {})) {
    const path = prefix ? `${prefix}.${name}` : name;
    const prev = out.get(path);
    if (!prev || (!prev.description && child.description)) out.set(path, { description: child.description ?? '' });
    leafFields(child, path, out);
    if (child.items) leafFields(child.items, `${path}[]`, out);
  }
  for (const key of ['anyOf', 'oneOf', 'allOf']) for (const branch of schema?.[key] ?? []) leafFields(branch, prefix, out);
  return out;
}

const isEnvelope = f => f.classes.length > 0 && f.classes.every(c => c === 'workflow-metadata');

const report = [];
for (const tool of contract.tools) {
  if (wanted.length && !wanted.includes(tool.name)) continue;
  const cov = coverage.tools[tool.name] ?? { evidence: [], fields: {} };
  const moduleDir = join(runtime, 'src/tools', DIR_OVERRIDES[tool.name] ?? snake(tool.name));
  const files = [...new Set([...rsFiles(moduleDir), ...cov.evidence.map(e => join(runtime, e)).flatMap(rsFiles)])];
  const corpus = files.map(f => readFileSync(f, 'utf8')).join('\n');
  const fields = [...leafFields(tool.querySchema)].map(([path, meta]) => {
    const leaf = path.split('.').pop().replace(/\[\]$/, '');
    const pattern = new RegExp(`\\b(${leaf}|${snake(leaf)})\\b`, 'g');
    return {
      path,
      classes: cov.fields[path] ?? [],
      hits: (corpus.match(pattern) ?? []).length,
      descriptionChars: meta.description.length,
    };
  });
  const declared = new Set(Object.keys(cov.fields));
  report.push({
    tool: tool.name,
    shortDescription: tool.shortDescription,
    variants: tool.variants.map(v => v.name),
    moduleDir: relative(root, moduleDir),
    evidenceFiles: files.map(f => relative(root, f)),
    fields,
    // Envelope fields (goal/reasoning/debug) are handled by the shared runtime, not tool modules.
    zeroHitFields: fields.filter(f => f.hits === 0 && !isEnvelope(f)).map(f => f.path),
    unclassifiedFields: fields.filter(f => !declared.has(f.path)).map(f => f.path),
    undescribedFields: fields.filter(f => f.descriptionChars === 0).map(f => f.path),
  });
}

if (json) {
  process.stdout.write(`${JSON.stringify({ fingerprint: contract.fingerprint, tools: report }, null, 2)}\n`);
} else {
  for (const t of report) {
    console.log(`\n## ${t.tool} — ${t.shortDescription}`);
    console.log(`module: ${t.moduleDir}  files: ${t.evidenceFiles.length}  fields: ${t.fields.length}  variants: ${t.variants.join(', ') || '-'}`);
    console.log(`zero-hit (candidate unused/misnamed): ${t.zeroHitFields.join(', ') || 'none'}`);
    console.log(`unclassified in field-effect-coverage: ${t.unclassifiedFields.join(', ') || 'none'}`);
    console.log(`no field description (may be carried by a parent/branch): ${t.undescribedFields.length ? t.undescribedFields.join(', ') : 'none'}`);
  }
}
