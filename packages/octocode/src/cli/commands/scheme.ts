// Node-owned `scheme`: the same composition the MCP server ships — runtime
// truth (availability, enforcement fingerprint) from the native binary's
// machine catalog, contract content (schemas, descriptions, instructions)
// from @octocodeai/octocode-core — reconciled by a fail-closed fingerprint
// check. The binary keeps only the machine catalog; all presentation is
// core-delivered here.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { ParsedArgs } from '../types.js';
import { EXIT } from '../exit-codes.js';
import { resolveNativeBin } from '../native-delegate.js';

const execFileAsync = promisify(execFile);

type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };
type JsonObject = { [key: string]: JsonValue };

export type SchemeView = 'full' | 'query';

interface MachineToolEntry {
  name: string;
  fields?: string;
  availability?: JsonValue;
}

interface MachineCatalog {
  fingerprint: string;
  tools: MachineToolEntry[];
}

const USAGE = `octocode scheme [toolName] [--view full|query] [--select FIELD=VALUE] [--compact]

  octocode scheme                   list every tool, availability, and agent instructions
  octocode scheme <toolName>        print the tool's contract
  octocode scheme <toolName> --view query [--select FIELD=VALUE]
                                    self-contained query schema, optionally isolated
                                    to one union branch`;

function writeJson(value: unknown, compact: boolean): number {
  console.log(compact ? JSON.stringify(value) : JSON.stringify(value, null, 2));
  return EXIT.OK;
}

/** Availability + enforcement fingerprint are runtime truth: ask the binary. */
async function readMachineCatalog(bin: string): Promise<MachineCatalog> {
  const { stdout } = await execFileAsync(bin, ['scheme', '--compact'], {
    maxBuffer: 4 * 1024 * 1024,
  });
  const catalog = JSON.parse(stdout) as {
    fingerprint?: unknown;
    tools?: unknown;
  };
  if (typeof catalog.fingerprint !== 'string' || !Array.isArray(catalog.tools)) {
    throw new Error('Native machine catalog is missing fingerprint or tools.');
  }
  return catalog as unknown as MachineCatalog;
}

function jsonType(value: JsonValue | undefined): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

/**
 * JSON Schema equates numeric spellings such as 1 and 1.0, including in
 * compound values. Only discard siblings when scalar disjointness is certain.
 */
function constsDisjoint(left: JsonValue, right: JsonValue): boolean {
  if (typeof left === 'string' && typeof right === 'string') return left !== right;
  if (typeof left === 'boolean' && typeof right === 'boolean') return left !== right;
  return jsonType(left) !== jsonType(right);
}

function deepEqual(left: JsonValue | undefined, right: JsonValue): boolean {
  return left !== undefined && JSON.stringify(left) === JSON.stringify(right);
}

function collectRefs(value: JsonValue | undefined, refs: string[]): void {
  if (Array.isArray(value)) {
    for (const entry of value) collectRefs(entry, refs);
    return;
  }
  if (!value || typeof value !== 'object') return;
  for (const [key, child] of Object.entries(value)) {
    if (
      (key === '$ref' || key === '$dynamicRef' || key === '$recursiveRef') &&
      typeof child === 'string'
    ) {
      refs.push(child);
    } else {
      collectRefs(child, refs);
    }
  }
}

function pruneUnreachableDefs(schema: JsonObject): void {
  const defs = schema.$defs;
  if (!defs || typeof defs !== 'object' || Array.isArray(defs)) return;
  const roots: JsonObject = { ...schema };
  delete roots.$defs;
  const pending: string[] = [];
  collectRefs(roots, pending);
  const needed = new Set<string>();
  while (pending.length > 0) {
    const reference = pending.pop() as string;
    // Anchor/external reference scopes can depend on definitions without a
    // JSON pointer. Keep all definitions when reachability is not provable.
    if (!reference.startsWith('#/$defs/')) return;
    const token = reference.slice('#/$defs/'.length).split('/')[0];
    if (token === undefined) return;
    const name = token.replaceAll('~1', '/').replaceAll('~0', '~');
    if (needed.has(name)) continue;
    needed.add(name);
    const definition = (defs as JsonObject)[name];
    if (definition === undefined) return;
    collectRefs(definition, pending);
  }
  if (needed.size === 0) {
    delete schema.$defs;
    return;
  }
  for (const name of Object.keys(defs as JsonObject)) {
    if (!needed.has(name)) delete (defs as JsonObject)[name];
  }
}

export function project(tool: JsonObject, view: SchemeView): JsonObject {
  if (view === 'full') {
    // The public catalog never carries outputSchema; drop defensively anyway.
    const { outputSchema: _outputSchema, ...published } = tool;
    return published;
  }
  // Keep the complete schema subtree: its local refs resolve against its
  // own root, including all core-owned $defs and validation constraints.
  const query: JsonObject = { name: tool.name, querySchema: tool.querySchema };
  if (tool.description !== undefined) query.description = tool.description;
  const inputSchema = tool.inputSchema;
  const queries =
    inputSchema && typeof inputSchema === 'object' && !Array.isArray(inputSchema)
      ? (inputSchema as JsonObject).properties &&
        ((inputSchema as JsonObject).properties as JsonObject).queries
      : undefined;
  if (queries && typeof queries === 'object' && !Array.isArray(queries)) {
    const bounds: JsonObject = {};
    for (const key of ['minItems', 'maxItems']) {
      const bound = (queries as JsonObject)[key];
      if (bound !== undefined) bounds[key] = bound;
    }
    if (Object.keys(bounds).length > 0) {
      query.queryEnvelope = { queries: bounds };
    }
  }
  return query;
}

function parseSelection(selection: string): [string, JsonValue] {
  const separator = selection.indexOf('=');
  const field = separator < 0 ? '' : selection.slice(0, separator);
  const raw = separator < 0 ? '' : selection.slice(separator + 1);
  if (field.trim() === '' || raw.trim() === '') {
    throw new Error('--select expects FIELD=VALUE, e.g. operation=code');
  }
  let value: JsonValue;
  try {
    value = JSON.parse(raw) as JsonValue;
  } catch {
    value = raw;
  }
  return [field, value];
}

export function projectSelected(
  tool: JsonObject,
  view: SchemeView,
  selection: string | undefined,
): JsonObject {
  if (selection === undefined) return project(tool, view);
  if (view !== 'query' || typeof tool.name !== 'string') {
    throw new Error('--select requires --view query and a tool name');
  }
  const [field, value] = parseSelection(selection);
  const projected = project(tool, view);
  const schema = projected.querySchema;
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) {
    throw new Error('Query schema must be an object');
  }
  const constOf = (branch: JsonValue): JsonValue | undefined => {
    if (!branch || typeof branch !== 'object' || Array.isArray(branch)) return undefined;
    const properties = (branch as JsonObject).properties;
    if (!properties || typeof properties !== 'object' || Array.isArray(properties)) {
      return undefined;
    }
    const property = (properties as JsonObject)[field];
    if (!property || typeof property !== 'object' || Array.isArray(property)) {
      return undefined;
    }
    return (property as JsonObject).const;
  };
  const candidates: Array<['oneOf' | 'anyOf', number]> = [];
  for (const union of ['oneOf', 'anyOf'] as const) {
    const branches = (schema as JsonObject)[union];
    if (!Array.isArray(branches)) continue;
    branches.forEach((branch, index) => {
      if (deepEqual(constOf(branch), value)) candidates.push([union, index]);
    });
  }
  if (candidates.length !== 1) {
    throw new Error(
      `--select "${selection}" matched ${candidates.length} top-level oneOf/anyOf branches; choose a const field/value identifying exactly one branch in --view query.`,
    );
  }
  const [union, index] = candidates[0];
  const branches = (schema as JsonObject)[union] as JsonValue[];
  const selected = branches[index];
  // Removing other oneOf branches must not admit instances that previously
  // matched multiple branches. Const discriminators usually prove disjointness;
  // retain exclusion constraints for siblings whose overlap cannot be ruled out.
  const requiresField = (branch: JsonValue): boolean => {
    if (!branch || typeof branch !== 'object' || Array.isArray(branch)) return false;
    const required = (branch as JsonObject).required;
    return Array.isArray(required) && required.some(name => name === field);
  };
  const overlaps: JsonValue[] = [];
  if (union === 'oneOf') {
    branches.forEach((branch, i) => {
      if (i === index) return;
      const other = constOf(branch);
      const provablyDisjoint =
        other !== undefined &&
        constsDisjoint(other, value) &&
        (requiresField(selected) || requiresField(branch));
      if (!provablyDisjoint) overlaps.push(branch);
    });
  }
  (schema as JsonObject)[union] = [selected];
  if (overlaps.length > 0) {
    const target = schema as JsonObject;
    if (!Array.isArray(target.allOf)) target.allOf = [];
    (target.allOf as JsonValue[]).push({ not: { anyOf: overlaps } });
  }
  pruneUnreachableDefs(schema as JsonObject);
  return projected;
}

function emitError(message: string, jsonErrors: boolean): void {
  if (jsonErrors) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.error(message);
  }
}

export async function runScheme(args: ParsedArgs): Promise<number> {
  const jsonErrors = args.options['json-errors'] === true;
  if (args.options.help === true || args.options.h === true) {
    console.log(USAGE);
    return EXIT.OK;
  }
  const compact = args.options.compact === true;
  const viewOption = args.options.view;
  if (viewOption !== undefined && viewOption !== 'full' && viewOption !== 'query') {
    emitError(`--view expects full|query, got: ${String(viewOption)}`, jsonErrors);
    return EXIT.USAGE;
  }
  const view: SchemeView = viewOption === 'query' ? 'query' : 'full';
  const select = typeof args.options.select === 'string' ? args.options.select : undefined;
  const toolName = args.args[0];

  const bin = resolveNativeBin();
  if (!bin) {
    emitError(
      'The native Octocode runtime is unavailable for this platform or installation.',
      jsonErrors,
    );
    return EXIT.TOOL;
  }

  let machine: MachineCatalog;
  try {
    machine = await readMachineCatalog(bin);
  } catch (error) {
    emitError(
      `Failed to read the native tool catalog: ${error instanceof Error ? error.message : String(error)}`,
      jsonErrors,
    );
    return EXIT.TOOL;
  }

  const { getPublicToolCatalog } = await import('@octocodeai/octocode-core/schema');
  const { buildMcpInstructions } = await import('@octocodeai/octocode-core/mcp');
  const catalog = getPublicToolCatalog();

  // Registration content comes from core while validation runs against the
  // native enforcement embed; refuse to describe tools a drifted runtime
  // would reject. Same gate and escape hatch as the MCP server.
  if (catalog.fingerprint !== machine.fingerprint) {
    const drift =
      `Contract drift: @octocodeai/octocode-core fingerprint ${catalog.fingerprint.slice(0, 12)}… ` +
      `does not match the native runtime fingerprint ${machine.fingerprint.slice(0, 12)}…. ` +
      'Reinstall matching octocode packages, or set OCTOCODE_ALLOW_CONTRACT_DRIFT=1 to bypass.';
    if (process.env.OCTOCODE_ALLOW_CONTRACT_DRIFT === '1') {
      console.error(`WARNING: ${drift}`);
    } else {
      emitError(drift, jsonErrors);
      return EXIT.TOOL;
    }
  }

  const machineByName = new Map(machine.tools.map(tool => [tool.name, tool]));

  if (toolName === undefined) {
    const enabled = machine.tools
      .filter(tool => {
        const availability = tool.availability;
        return (
          !!availability &&
          typeof availability === 'object' &&
          !Array.isArray(availability) &&
          (availability as JsonObject).enabled === true
        );
      })
      .map(tool => tool.name);
    const tools = (catalog.tools as readonly JsonObject[]).map(tool => {
      const name = String(tool.name);
      const runtimeEntry = machineByName.get(name);
      return {
        name,
        description: tool.shortDescription ?? '',
        fields: runtimeEntry?.fields ?? '[]',
        availability: runtimeEntry?.availability ?? { enabled: false },
      };
    });
    return writeJson(
      {
        kind: 'octocode.toolCatalog',
        version: 1,
        toolCount: tools.length,
        output:
          'Compact discovery catalog with availability-scoped agent instructions. Inspect one tool before execution.',
        commands: {
          schema: 'scheme <name>',
          querySchema: 'scheme <name> --view query',
          run: "<name> '<json>'",
        },
        instructions: buildMcpInstructions(enabled),
        tools,
      },
      compact,
    );
  }

  const tool = (catalog.tools as readonly JsonObject[]).find(
    candidate => candidate.name === toolName,
  );
  if (!tool) {
    const known = (catalog.tools as readonly JsonObject[])
      .map(candidate => String(candidate.name))
      .join(', ');
    emitError(`Unknown tool: ${toolName}. Known tools: ${known}`, jsonErrors);
    return EXIT.USAGE;
  }
  let value: JsonObject;
  try {
    value = projectSelected({ ...tool }, view, select);
  } catch (error) {
    emitError(error instanceof Error ? error.message : String(error), jsonErrors);
    return EXIT.USAGE;
  }
  // The compact catalog carries a generic `run` hint; the per-tool view echoes
  // the concrete invocation so an agent inspecting one contract sees exactly
  // how to execute it.
  value.run = `octocode ${toolName} '<json>'`;
  return writeJson(value, compact);
}

export const schemeCommand = {
  name: 'scheme',
  handler: async (args: ParsedArgs): Promise<void> => {
    process.exitCode = await runScheme(args);
  },
};
