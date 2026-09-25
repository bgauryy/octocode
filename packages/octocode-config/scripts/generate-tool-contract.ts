// The ONE place tool contracts are generated. Every consumer (MCP, CLI, and
// octocode-native) reads these outputs; nothing else generates or copies them.
//
//   core Zod schemas ── buildEnforcementContractIr ──▶ contract/tool-contract.json
//        │                                             (native embed: schemas, rules, defaults)
//        ├── buildNativeParityFixtures ───────────────▶ contract/contract-fixtures.json
//        └── bundle ──▶ contract/tool-types.schema.json
//                 ├── json-schema-to-typescript ──▶ src/contracts/toolTypes.generated.ts
//                 └── cargo-typify 0.8.0 ─────────▶ contract/tool_types.rs
//
// octocode-native's build.rs embeds contract/tool-contract.json and include!s
// contract/tool_types.rs directly from this package, so regenerating here is
// the whole change: native rebuilds against it with no copy, pin, or script.
// Outputs are committed and never hand-edited; `--check` fails when stale.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  buildEnforcementContractIr,
  buildNativeParityFixtures,
  canonicalizeNativeContractIr,
  type EnforcementContractIr,
} from '@octocodeai/octocode-core/schema';
import { compile } from 'json-schema-to-typescript';

const TYPIFY_VERSION = '0.8.0';
const contractDir = (name: string): string =>
  fileURLToPath(new URL(`../contract/${name}`, import.meta.url));
const contractPath = contractDir('tool-contract.json');
const fixturesPath = contractDir('contract-fixtures.json');
const provenancePath = contractDir('provenance.json');
const bundlePath = contractDir('tool-types.schema.json');
const tsPath = fileURLToPath(
  new URL('../src/contracts/toolTypes.generated.ts', import.meta.url)
);
const rustPath = contractDir('tool_types.rs');

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type JsonObject = { [key: string]: Json };
type ToolRecord = {
  name: string;
  querySchema: JsonObject;
  inputSchema: JsonObject;
  outputSchema: JsonObject;
};

const pascal = (value: string): string => value.charAt(0).toUpperCase() + value.slice(1);
const defName = (value: string): string => value.replace(/^_+/, '');

function sortKeys(value: Json): Json {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, sortKeys(value[key] as Json)])
    );
  }
  return value;
}

function withoutAnnotations(value: Json): Json {
  if (Array.isArray(value)) return value.map(withoutAnnotations);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== 'title' && key !== 'description')
      .map(([key, child]) => [key, withoutAnnotations(child)])
  );
}

function sameShape(left: Json, right: Json): boolean {
  return (
    JSON.stringify(sortKeys(withoutAnnotations(left))) ===
    JSON.stringify(sortKeys(withoutAnnotations(right)))
  );
}

/**
 * Rewrites local `#/$defs/<name>` refs to bundle-level names and prefixes
 * nested titles with the owning type so generated names cannot collide across
 * tools (lspSearch "Anchored" → "LspSearchQueryAnchored").
 */
function relocate(value: Json, rename: (local: string) => string, owner: string): Json {
  if (Array.isArray(value)) return value.map((item) => relocate(item, rename, owner));
  if (!value || typeof value !== 'object') return value;
  const result: JsonObject = {};
  for (const [key, child] of Object.entries(value)) {
    if (key === '$ref' && typeof child === 'string' && child.startsWith('#/$defs/')) {
      result[key] = `#/$defs/${rename(child.slice('#/$defs/'.length))}`;
    } else if (key === 'title' && typeof child === 'string') {
      result[key] = `${owner}${pascal(child)}`;
    } else {
      result[key] = relocate(child, rename, owner);
    }
  }
  return result;
}

const isStringEnum = (schema: Json): schema is JsonObject =>
  !!schema &&
  typeof schema === 'object' &&
  !Array.isArray(schema) &&
  schema.type === 'string' &&
  Array.isArray(schema.enum) &&
  Object.keys(schema).every((key) => ['type', 'enum', 'description', 'default'].includes(key));

/**
 * Property enums become named shared types so tools that speak the same
 * vocabulary (chunkType, minify, caseMode, …) share one Rust/TS enum instead
 * of a structurally identical copy per tool. A property name maps to one
 * shared shape: its only shape, or else the single shape used by two or more
 * tools. Other shapes under that name stay inline and tool-specific.
 */
function extractSharedEnums(defs: Record<string, Json>): void {
  type Visitor = (name: string, schema: JsonObject, parent: JsonObject) => void;
  const visit = (value: Json, onProperty: Visitor): void => {
    if (Array.isArray(value)) return value.forEach((item) => visit(item, onProperty));
    if (!value || typeof value !== 'object') return;
    const properties = value.properties;
    if (properties && typeof properties === 'object' && !Array.isArray(properties)) {
      for (const [name, schema] of Object.entries(properties)) {
        if (isStringEnum(schema)) onProperty(name, schema, properties);
      }
    }
    Object.values(value).forEach((child) => visit(child, onProperty));
  };
  const enumShape = (schema: JsonObject): string => JSON.stringify(schema.enum);
  const owner = (defName: string): string => defName.replace(/(?:Query|Input|Output).*$/, '');
  // property name → enum shape → tools whose query/input types use it
  const usage = new Map<string, Map<string, Set<string>>>();
  for (const [defName, schema] of Object.entries(defs)) {
    visit(schema, (name, property) => {
      const byShape = usage.get(name) ?? new Map<string, Set<string>>();
      const tools = byShape.get(enumShape(property)) ?? new Set<string>();
      if (!defName.startsWith('Shared') && !/Output/.test(defName)) tools.add(owner(defName));
      byShape.set(enumShape(property), tools);
      usage.set(name, byShape);
    });
  }
  const shared = new Map<string, { typeName: string; shape: string }>();
  for (const [name, byShape] of usage) {
    const typeName = pascal(name);
    if (typeName in defs) continue;
    const candidates =
      byShape.size === 1 ? [...byShape.keys()] : [...byShape].filter(([, tools]) => tools.size >= 2).map(([shape]) => shape);
    if (candidates.length === 1) shared.set(name, { typeName, shape: candidates[0] as string });
  }
  for (const schema of Object.values(defs)) {
    visit(schema, (name, property, parent) => {
      const target = shared.get(name);
      if (!target || enumShape(property) !== target.shape) return;
      defs[target.typeName] = { title: target.typeName, type: 'string', enum: property.enum as Json };
      const { type: _type, enum: _enum, ...annotations } = property;
      parent[name] = { $ref: `#/$defs/${target.typeName}`, ...annotations };
    });
  }
}

export function buildToolTypesBundle(ir: EnforcementContractIr = buildEnforcementContractIr()): {
  fingerprint: string;
  bundle: JsonObject;
} {
  const tools = ir.tools as unknown as ToolRecord[];
  const defs: Record<string, Json> = {};
  const add = (name: string, schema: Json): void => {
    const existing = defs[name];
    if (existing !== undefined && JSON.stringify(sortKeys(existing)) !== JSON.stringify(sortKeys(schema))) {
      throw new Error(`Conflicting tool-type definition: ${name}`);
    }
    defs[name] = schema;
  };
  const register = (name: string, source: JsonObject, shared: boolean): void => {
    const { $schema: _schema, $defs: inner = {}, title: _title, ...body } = source;
    // Output $defs are identical across tools (continuations, AST rules) and
    // become one Shared* type; query/input $defs stay namespaced per type.
    const rename = (local: string): string =>
      shared ? `Shared${pascal(defName(local))}` : `${name}${pascal(defName(local))}`;
    for (const [local, schema] of Object.entries(inner as JsonObject)) {
      add(rename(local), { ...(relocate(schema, rename, rename(local)) as JsonObject), title: rename(local) });
    }
    add(name, { ...(relocate(body, rename, name) as JsonObject), title: name });
  };
  for (const tool of tools) {
    const base = pascal(tool.name);
    register(`${base}Query`, tool.querySchema, false);
    register(`${base}Input`, tool.inputSchema, false);
    register(`${base}Output`, tool.outputSchema, true);
    // The bulk envelope usually repeats the row schema inline; reference the
    // Query type instead so both languages expose one row type, not two.
    const input = defs[`${base}Input`] as JsonObject;
    const queries = (input.properties as JsonObject | undefined)?.queries as JsonObject | undefined;
    if (queries?.items && sameShape(queries.items, defs[`${base}Query`] as Json)) {
      queries.items = { $ref: `#/$defs/${base}Query` };
    }
  }
  extractSharedEnums(defs);
  const names = Object.keys(defs).sort();
  const bundle: JsonObject = {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    $comment: `@generated by @octocodeai/config scripts/generate-tool-contract.ts from contract ${ir.fingerprint}; do not edit.`,
    title: 'OctocodeToolTypes',
    $defs: Object.fromEntries(names.map((name) => [name, defs[name] as Json])),
  };
  return { fingerprint: ir.fingerprint, bundle };
}

async function renderTypeScript(fingerprint: string, bundle: JsonObject): Promise<string> {
  const defs = bundle.$defs as JsonObject;
  const tools = Object.keys(defs).filter((name) => /(?:Query|Input|Output)$/.test(name));
  // A root that references every top-level type makes the compiler declare
  // each one under its own name.
  const root = {
    ...bundle,
    type: 'object',
    additionalProperties: false,
    properties: Object.fromEntries(tools.map((name) => [name, { $ref: `#/$defs/${name}` }])),
  };
  const body = await compile(root as never, 'OctocodeToolTypes', {
    bannerComment: '',
    additionalProperties: false,
    declareExternallyReferenced: true,
    enableConstEnums: false,
    ignoreMinAndMaxItems: true,
    format: false,
    strictIndexSignatures: true,
    unreachableDefinitions: true,
  });
  const toolNames = [...new Set(tools.map((name) => name.replace(/(?:Query|Input|Output)$/, '')))];
  const map = (suffix: string): string =>
    toolNames
      .map((tool) => `  ${tool.charAt(0).toLowerCase()}${tool.slice(1)}: ${tool}${suffix};`)
      .join('\n');
  return `// @generated by @octocodeai/config scripts/generate-tool-contract.ts; do not edit.
// Source: @octocodeai/octocode-core Zod contract ${fingerprint}.
/* eslint-disable */

export const TOOL_TYPES_CONTRACT_FINGERPRINT = ${JSON.stringify(fingerprint)};

${body.replace(/^export interface OctocodeToolTypes \{[\s\S]*?\n\}\n/m, '').trim()}

/** One validated query row, keyed by tool name. */
export interface ToolQueryMap {
${map('Query')}
}
/** Bulk input envelope accepted by each tool, keyed by tool name. */
export interface ToolInputMap {
${map('Input')}
}
/** Structured output envelope returned by each tool, keyed by tool name. */
export interface ToolOutputMap {
${map('Output')}
}
export type ToolTypeName = keyof ToolQueryMap;
export type ToolQuery<N extends ToolTypeName> = ToolQueryMap[N];
export type ToolInput<N extends ToolTypeName> = ToolInputMap[N];
export type ToolOutput<N extends ToolTypeName> = ToolOutputMap[N];
`;
}

const rustHeader = (fingerprint: string, schemaSha256: string): string =>
  `// @generated by @octocodeai/config scripts/generate-tool-contract.ts (cargo-typify ${TYPIFY_VERSION}); do not edit.
// Source: @octocodeai/octocode-core Zod contract ${fingerprint}.
pub const TOOL_TYPES_CONTRACT_FINGERPRINT: &str = ${JSON.stringify(fingerprint)};
pub const TOOL_TYPES_SCHEMA_SHA256: &str = ${JSON.stringify(schemaSha256)};
`;

function renderRust(header: string, bundleJson: string): string {
  const scratch = mkdtempSync(join(tmpdir(), 'octocode-tool-types-'));
  try {
    const input = join(scratch, 'tool-types.schema.json');
    const output = join(scratch, 'tool_types.rs');
    writeFileSync(input, bundleJson);
    let version = '';
    try {
      version = execFileSync('cargo', ['typify', '--version'], { encoding: 'utf8' });
    } catch {
      throw new Error(`cargo-typify is required: cargo install cargo-typify --version ${TYPIFY_VERSION} --locked`);
    }
    if (!version.includes(TYPIFY_VERSION)) {
      throw new Error(`cargo-typify ${TYPIFY_VERSION} is required (found ${version.trim()}).`);
    }
    execFileSync('cargo', ['typify', '--no-builder', '--additional-derive', 'PartialEq', input, '-o', output], {
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    // Inner attributes cannot survive `include!`; the native wrapper module
    // carries the equivalent `#[allow]`s.
    const generated = readFileSync(output, 'utf8')
      .split('\n')
      .filter((line) => !line.startsWith('#![') && !line.startsWith('//!'))
      .join('\n')
      .trim();
    // Validated string newtypes are still strings to their consumers: let them
    // format like the `String` they wrap (Deref already lends `&String`).
    const newtypes = [
      ...generated.matchAll(/^pub struct (\w+)\(::std::string::String\);$/gm),
    ].map((match) => match[1]);
    const stringImpls = newtypes
      .map(
        (name) => `impl ::std::fmt::Display for ${name} {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        f.write_str(&self.0)
    }
}`
      )
      .join('\n');
    return `${header}\n${generated}\n${stringImpls}\n`;
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

/**
 * Bundle + TypeScript outputs, plus the header the Rust file must start with.
 * The header pins the bundle digest, so staleness is checkable without cargo.
 */
export async function generateToolContract(): Promise<{
  files: Map<string, string>;
  bundleJson: string;
  rustHeader: string;
}> {
  const ir = buildEnforcementContractIr();
  const contractJson = canonicalizeNativeContractIr(ir);
  const { fingerprint, bundle } = buildToolTypesBundle(ir);
  const bundleJson = `${JSON.stringify(bundle, null, 2)}\n`;
  const sha256 = (text: string): string => createHash('sha256').update(text).digest('hex');
  const provenance = {
    sourcePackage: '@octocodeai/octocode-core',
    sourceVersion: corePackageVersion(),
    contractFormatVersion: ir.contractFormatVersion,
    contractFingerprint: fingerprint,
    contractSha256: sha256(contractJson),
  };
  return {
    files: new Map([
      [contractPath, contractJson],
      [fixturesPath, `${JSON.stringify(buildNativeParityFixtures(), null, 2)}\n`],
      [provenancePath, `${JSON.stringify(provenance, null, 2)}\n`],
      [bundlePath, bundleJson],
      [tsPath, await renderTypeScript(fingerprint, bundle)],
    ]),
    bundleJson,
    rustHeader: rustHeader(fingerprint, sha256(bundleJson)),
  };
}

function corePackageVersion(): string {
  // `./schema` resolves to <core>/dist/schema.js; the manifest sits one level up.
  const manifest = new URL('../package.json', import.meta.resolve('@octocodeai/octocode-core/schema'));
  return (JSON.parse(readFileSync(manifest, 'utf8')) as { version: string }).version;
}

async function main(): Promise<void> {
  const { files, bundleJson, rustHeader: header } = await generateToolContract();
  if (process.argv.includes('--check')) {
    const read = (path: string): string => {
      try {
        return readFileSync(path, 'utf8');
      } catch {
        return '';
      }
    };
    const stale = [...files].filter(([path, content]) => read(path) !== content).map(([path]) => path);
    if (!read(rustPath).startsWith(header)) stale.push(rustPath);
    if (stale.length > 0) {
      throw new Error(
        `Generated tool contract is stale (${stale.join(', ')}). Run: yarn workspace @octocodeai/config generate:tool-contract`
      );
    }
    return;
  }
  for (const [path, content] of files) writeFileSync(path, content);
  writeFileSync(rustPath, renderRust(header, bundleJson));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
