// Node-owned `schema`: the same composition the MCP server ships — runtime
// truth (availability, enforcement fingerprint) from the native binary's
// machine catalog, canonical contract content from @octocodeai/octocode-core,
// and capability-aware presentation addons from @octocodeai/config/schema —
// reconciled by a fail-closed core fingerprint check. The binary keeps only
// the machine catalog; config composes the presentation delivered here.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { ParsedArgs } from '../types.js';
import { EXIT } from '../exit-codes.js';
import { resolveNativeBin } from '../native-delegate.js';
import { contractDriftAllowed, contractDriftMessage } from '@octocodeai/config';
import type { GrammarCapability } from '@octocodeai/config/mcp';
import type { SchemaJsonObject, SchemaView } from '@octocodeai/config/schema';
import { project } from './schema-projection.js';

const execFileAsync = promisify(execFile);

// Replaced with `true` by the esbuild define in build.mjs; undefined when
// running from source (vitest, tsx).
declare const __OCTOCODE_BUNDLED__: boolean | undefined;

interface MachineToolEntry {
  name: string;
  fields?: string;
  availability?: unknown;
}

interface MachineCatalog {
  fingerprint: string;
  tools: MachineToolEntry[];
  grammarCapabilities?: GrammarCapability[];
}

/** A terminal gets indented JSON; a pipe gets one line. */
function writeJson(value: unknown): number {
  console.log(
    process.stdout.isTTY === true
      ? JSON.stringify(value, null, 2)
      : JSON.stringify(value)
  );
  return EXIT.OK;
}

/** Availability + enforcement fingerprint are runtime truth: ask the binary. */
async function readMachineCatalog(bin: string): Promise<MachineCatalog> {
  // Match delegateToNative: a `.cjs`/`.js` resolved bin is a Node launcher and
  // must run through `process.execPath`, not be exec'd directly (which fails).
  const isLauncher = bin.endsWith('.cjs') || bin.endsWith('.js');
  const [command, prefixArgs] = isLauncher
    ? [process.execPath, [bin]]
    : [bin, [] as string[]];
  const { stdout } = await execFileAsync(
    command,
    [...prefixArgs, 'catalog'],
    {
      maxBuffer: 4 * 1024 * 1024,
    }
  );
  const catalog = JSON.parse(stdout) as {
    fingerprint?: unknown;
    tools?: unknown;
  };
  if (
    typeof catalog.fingerprint !== 'string' ||
    !Array.isArray(catalog.tools)
  ) {
    throw new Error('Native machine catalog is missing fingerprint or tools.');
  }
  return catalog as unknown as MachineCatalog;
}

/**
 * Errors follow the output: the native CLI's JSON envelope on a pipe, text
 * on a terminal.
 */
function emitError(message: string): void {
  if (process.stdout.isTTY !== true) {
    console.log(
      JSON.stringify({ kind: 'octocode.toolError', version: 1, error: message })
    );
  } else {
    console.error(message);
  }
}

function isEnabled(availability: unknown): boolean {
  return (
    !!availability &&
    typeof availability === 'object' &&
    !Array.isArray(availability) &&
    (availability as SchemaJsonObject).enabled === true
  );
}

type ToolPresentation = {
  machine: MachineCatalog;
  catalog: ReturnType<
    typeof import('@octocodeai/config/schema').getPublicToolCatalogWithAddons
  >;
  enabled: string[];
};

async function loadPresentation(): Promise<
  ({ ok: true } & ToolPresentation) | { ok: false; exitCode: number }
> {
  const bin = resolveNativeBin();
  if (!bin) {
    emitError(
      'The native Octocode runtime is unavailable for this platform or installation.');
    return { ok: false, exitCode: EXIT.TOOL };
  }

  let machine: MachineCatalog;
  try {
    machine = await readMachineCatalog(bin);
  } catch (error) {
    emitError(
      `Failed to read the native tool catalog: ${error instanceof Error ? error.message : String(error)}`);
    return { ok: false, exitCode: EXIT.TOOL };
  }

  const { getPublicToolCatalogWithAddons } =
    await import('@octocodeai/config/schema');
  const enabled = machine.tools
    .filter(tool => isEnabled(tool.availability))
    .map(tool => tool.name);
  const catalog = getPublicToolCatalogWithAddons({
    availableTools: enabled,
  });

  // Discovery content is composed by config while validation runs against the
  // native enforcement embed; refuse to describe tools a drifted runtime
  // would reject. Same gate, message, and escape hatch as the MCP server.
  if (catalog.fingerprint !== machine.fingerprint) {
    const drift = contractDriftMessage(
      catalog.fingerprint,
      machine.fingerprint
    );
    const bundled =
      typeof __OCTOCODE_BUNDLED__ !== 'undefined' &&
      __OCTOCODE_BUNDLED__ === true;
    if (contractDriftAllowed(process.env, { bundled })) {
      console.error(`WARNING: ${drift}`);
    } else {
      emitError(drift);
      return { ok: false, exitCode: EXIT.TOOL };
    }
  }

  return { ok: true, machine, catalog, enabled };
}

function instructionsFor(presentation: ToolPresentation): Promise<string> {
  return import('@octocodeai/config/mcp').then(({ buildMcpInstructions }) =>
    buildMcpInstructions(presentation.enabled, {
      grammarCapabilities: presentation.machine.grammarCapabilities ?? [],
    })
  );
}

export async function runSchema(args: ParsedArgs): Promise<number> {
  const unknown = Object.keys(args.options).find(
    key => key !== 'view' && key !== 'select'
  );
  if (unknown) {
    emitError(
      `Unknown option: --${unknown}. Usage: octocode schema [tool] [--view query|variants|full] [--select FIELD=VALUE]`
    );
    return EXIT.USAGE;
  }
  for (const key of ['view', 'select']) {
    if (
      args.options[key] !== undefined &&
      typeof args.options[key] !== 'string'
    ) {
      emitError(`--${key} requires a value.`);
      return EXIT.USAGE;
    }
  }
  if (args.args.length > 1) {
    emitError('schema accepts one tool name per call.');
    return EXIT.USAGE;
  }
  const viewOption = args.options.view;
  if (
    viewOption !== undefined &&
    viewOption !== 'full' &&
    viewOption !== 'query' &&
    viewOption !== 'variants'
  ) {
    emitError(`--view expects query|variants|full, got: ${String(viewOption)}`);
    return EXIT.USAGE;
  }
  const view: SchemaView =
    viewOption === 'query'
      ? 'query'
      : viewOption === 'variants'
        ? 'variants'
        : 'full';
  const select =
    typeof args.options.select === 'string' ? args.options.select : undefined;
  const toolName = args.args[0];
  if (toolName === undefined && (viewOption ?? select) !== undefined) {
    emitError('--view and --select need a tool name: octocode schema <tool> --view query');
    return EXIT.USAGE;
  }

  const presentation = await loadPresentation();
  if (!presentation.ok) return presentation.exitCode;
  const { machine, catalog } = presentation;

  const machineByName = new Map(machine.tools.map(tool => [tool.name, tool]));

  // Discovery lists only tools this surface can run, like MCP tools/list.
  const listed = (catalog.tools as readonly SchemaJsonObject[]).filter(tool =>
    presentation.enabled.includes(String(tool.name))
  );
  if (toolName === undefined) {
    const tools = listed.map(tool => {
      const name = String(tool.name);
      const runtimeEntry = machineByName.get(name);
      return {
        name,
        description: tool.shortDescription ?? '',
        fields: runtimeEntry?.fields ?? '[]',
      };
    });
    return writeJson(
      {
        kind: 'octocode.toolCatalog',
        version: 1,
        toolCount: tools.length,
        commands: {
          run: "octocode <tool> '<json>' (or --input FILE|-)",
          query: 'octocode schema <tool> --view query [--select FIELD=VALUE]',
          variants: 'octocode schema <tool> --view variants',
          full: 'octocode schema <tool>',
        },
        instructions: await instructionsFor(presentation),
        tools,
      }
    );
  }

  const tool = (catalog.tools as readonly SchemaJsonObject[]).find(
    candidate => candidate.name === toolName
  );
  if (!tool) {
    const known = listed.map(candidate => String(candidate.name)).join(', ');
    emitError(`Unknown tool: ${toolName}. Known tools: ${known}`);
    return EXIT.USAGE;
  }
  // The catalog already carries the availability-scoped description.
  let value: SchemaJsonObject;
  try {
    value = project(tool, view, select);
  } catch (error) {
    emitError(error instanceof Error ? error.message : String(error));
    return EXIT.USAGE;
  }
  // Carry runtime availability into the per-tool view so an agent inspecting a
  // contract right before calling sees the gate (e.g. astRewrite's disabled
  // state and OCTOCODE_BETA env var). The core contract catalog has no
  // availability field; only the native machine catalog knows it, so join it
  // here exactly as the discovery list does above.
  const runtimeEntry = machineByName.get(String(toolName));
  value.availability = (runtimeEntry?.availability ?? {
    enabled: false,
  }) as SchemaJsonObject;
  // The compact catalog carries a generic `run` hint; the per-tool view echoes
  // the concrete invocation so an agent inspecting one contract sees exactly
  // how to execute it.
  value.run = `octocode ${toolName} '<json>'`;
  return writeJson(value);
}

export const schemaCommand = {
  name: 'schema',
  handler: async (args: ParsedArgs): Promise<void> => {
    process.exitCode = await runSchema(args);
  },
};
