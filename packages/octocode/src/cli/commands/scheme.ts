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
  if (
    typeof catalog.fingerprint !== 'string' ||
    !Array.isArray(catalog.tools)
  ) {
    throw new Error('Native machine catalog is missing fingerprint or tools.');
  }
  return catalog as unknown as MachineCatalog;
}

function emitError(message: string, jsonErrors: boolean): void {
  if (jsonErrors) {
    console.log(JSON.stringify({ success: false, error: message }));
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

export async function runScheme(args: ParsedArgs): Promise<number> {
  const jsonErrors = args.options['json-errors'] === true;
  if (args.options.help === true || args.options.h === true) {
    console.log(USAGE);
    return EXIT.OK;
  }
  const compact = args.options.compact === true;
  const viewOption = args.options.view;
  if (
    viewOption !== undefined &&
    viewOption !== 'full' &&
    viewOption !== 'query'
  ) {
    emitError(
      `--view expects full|query, got: ${String(viewOption)}`,
      jsonErrors
    );
    return EXIT.USAGE;
  }
  const view: SchemeView = viewOption === 'query' ? 'query' : 'full';
  const select =
    typeof args.options.select === 'string' ? args.options.select : undefined;
  const toolName = args.args[0];

  const bin = resolveNativeBin();
  if (!bin) {
    emitError(
      'The native Octocode runtime is unavailable for this platform or installation.',
      jsonErrors
    );
    return EXIT.TOOL;
  }

  let machine: MachineCatalog;
  try {
    machine = await readMachineCatalog(bin);
  } catch (error) {
    emitError(
      `Failed to read the native tool catalog: ${error instanceof Error ? error.message : String(error)}`,
      jsonErrors
    );
    return EXIT.TOOL;
  }

  const { getPublicToolCatalog } = await import('@octocodeai/config/schema');
  const { buildMcpInstructions } = await import('@octocodeai/config/mcp');
  const catalog = getPublicToolCatalog();

  // Discovery content comes from core while validation runs against the
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
      .filter(tool => isEnabled(tool.availability))
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
    value = projectSelected({ ...tool }, view, select);
  } catch (error) {
    emitError(
      error instanceof Error ? error.message : String(error),
      jsonErrors
    );
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
