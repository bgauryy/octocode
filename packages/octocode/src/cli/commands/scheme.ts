// Node-owned `scheme`: the same composition the MCP server ships — runtime
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
import type { GrammarCapability } from '@octocodeai/config/mcp';
import {
  projectSelected,
  type JsonObject,
  type SchemeView,
} from './scheme-projection.js';

export { project, projectSelected } from './scheme-projection.js';

const execFileAsync = promisify(execFile);

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

const USAGE = `octocode scheme [toolName] [--view full|query|variants] [--select FIELD=VALUE] [--compact|--pretty]

  octocode scheme                   list every tool, availability, and agent instructions
  octocode scheme <toolName>        print the tool's contract with variants before its schema
  octocode scheme <toolName> --view variants
                                    compact branch names, selectors, and examples
  octocode scheme <toolName> --view query [--select variant=NAME]
                                    self-contained query schema, optionally isolated
                                    to one named variant or const field/value`;

function writeJson(value: unknown, compact: boolean): number {
  console.log(compact ? JSON.stringify(value) : JSON.stringify(value, null, 2));
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
    [...prefixArgs, 'scheme', '--compact'],
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

/** Same `--json-errors` envelope as the native CLI and contract input errors. */
function emitError(message: string, jsonErrors: boolean): void {
  if (jsonErrors) {
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
    (availability as JsonObject).enabled === true
  );
}

/**
 * Agents read scheme through pipes: default to single-line JSON there, like
 * tool output. A terminal keeps the indented view unless --compact is set;
 * --pretty forces indentation anywhere.
 */
export function useCompactJson(
  options: ParsedArgs['options'],
  isTty: boolean
): boolean {
  if (options.compact === true) return true;
  if (options.pretty === true) return false;
  return !isTty;
}

type ToolPresentation = {
  machine: MachineCatalog;
  catalog: ReturnType<
    typeof import('@octocodeai/config/schema').getPublicToolCatalogWithAddons
  >;
  enabled: string[];
};

async function loadPresentation(
  jsonErrors: boolean
): Promise<
  ({ ok: true } & ToolPresentation) | { ok: false; exitCode: number }
> {
  const bin = resolveNativeBin();
  if (!bin) {
    emitError(
      'The native Octocode runtime is unavailable for this platform or installation.',
      jsonErrors
    );
    return { ok: false, exitCode: EXIT.TOOL };
  }

  let machine: MachineCatalog;
  try {
    machine = await readMachineCatalog(bin);
  } catch (error) {
    emitError(
      `Failed to read the native tool catalog: ${error instanceof Error ? error.message : String(error)}`,
      jsonErrors
    );
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
  // would reject. Same gate and escape hatch as the MCP server.
  if (catalog.fingerprint !== machine.fingerprint) {
    const drift =
      `Contract drift: @octocodeai/octocode-core fingerprint ${catalog.fingerprint.slice(0, 12)}… ` +
      `does not match the native runtime fingerprint ${machine.fingerprint.slice(0, 12)}…. ` +
      'Reinstall matching octocode packages (in the repo: `yarn contracts:regen` and rebuild native), or set OCTOCODE_ALLOW_CONTRACT_DRIFT=1 to bypass (ignored when NODE_ENV=production).';
    // Same gate as the MCP server: the override is a local-iteration aid and
    // never applies in production.
    if (
      process.env.OCTOCODE_ALLOW_CONTRACT_DRIFT === '1' &&
      process.env.NODE_ENV !== 'production'
    ) {
      console.error(`WARNING: ${drift}`);
    } else {
      emitError(drift, jsonErrors);
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

/** Root help shares the catalog's runtime availability and drift checks. */
export async function printAgentInstructions(): Promise<number> {
  const presentation = await loadPresentation(false);
  if (!presentation.ok) return presentation.exitCode;
  console.log(`\nAgent instructions:\n${await instructionsFor(presentation)}`);
  return EXIT.OK;
}

export async function runScheme(args: ParsedArgs): Promise<number> {
  const jsonErrors = args.options['json-errors'] === true;
  const allowed = new Set([
    'help',
    'h',
    'view',
    'select',
    'compact',
    'pretty',
    'json-errors',
    'no-color',
    'redact-emails',
  ]);
  const unknown = Object.keys(args.options).find(key => !allowed.has(key));
  if (unknown) {
    emitError(`Unknown option: --${unknown}`, jsonErrors);
    return EXIT.USAGE;
  }
  for (const key of ['view', 'select']) {
    if (
      args.options[key] !== undefined &&
      typeof args.options[key] !== 'string'
    ) {
      emitError(`--${key} requires a value.`, jsonErrors);
      return EXIT.USAGE;
    }
  }
  if (args.options.help === true || args.options.h === true) {
    console.log(USAGE);
    return EXIT.OK;
  }
  if (args.args.length > 1) {
    emitError('scheme accepts one tool name per call.', jsonErrors);
    return EXIT.USAGE;
  }
  const compact = useCompactJson(args.options, process.stdout.isTTY === true);
  const viewOption = args.options.view;
  if (
    viewOption !== undefined &&
    viewOption !== 'full' &&
    viewOption !== 'query' &&
    viewOption !== 'variants'
  ) {
    emitError(
      `--view expects full|query|variants, got: ${String(viewOption)}`,
      jsonErrors
    );
    return EXIT.USAGE;
  }
  const view: SchemeView =
    viewOption === 'query'
      ? 'query'
      : viewOption === 'variants'
        ? 'variants'
        : 'full';
  const select =
    typeof args.options.select === 'string' ? args.options.select : undefined;
  const toolName = args.args[0];

  const presentation = await loadPresentation(jsonErrors);
  if (!presentation.ok) return presentation.exitCode;
  const { machine, catalog, enabled } = presentation;
  const { getDirectToolDefinitionsWithAddons } =
    await import('@octocodeai/config/schema');

  const machineByName = new Map(machine.tools.map(tool => [tool.name, tool]));

  if (toolName === undefined) {
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
          schema: 'scheme <name> --view query',
          fullContract: 'scheme <name> --view full',
          variants: 'scheme <name> --view variants',
          querySchema: 'scheme <name> --view query [--select variant=<name>]',
          run: "<name> '<json>'",
        },
        instructions: await instructionsFor(presentation),
        tools,
      },
      compact
    );
  }

  const tool = (catalog.tools as readonly JsonObject[]).find(
    candidate => candidate.name === toolName
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
    const definition = getDirectToolDefinitionsWithAddons({
      availableTools: enabled,
    }).find(candidate => candidate.name === toolName);
    value = projectSelected(
      { ...tool, description: definition?.description ?? tool.description },
      view,
      select
    );
  } catch (error) {
    emitError(
      error instanceof Error ? error.message : String(error),
      jsonErrors
    );
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
  }) as JsonObject;
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
