/**
 * registerUniqueTool — shared helper used by all extension tool registrations.
 *
 * Native Octocode research tools (GitHub, local, LSP, npm) are no longer registered
 * as individual Pi tools. They are served via the bundled octocode MCP server through
 * MCPTool. The shared catalog owns the available research capabilities.
 * Full MCP discovery runs at session_start via warmMcpCatalog() and
 * before_agent_start awaits it (mcpCatalogReady). By default the first system
 * prompt receives the schema-aware <mcp_catalog_index>; OCTOCODE_COMPACT_MCP=0 opts
 * into the unoptimized exact catalog projection for debugging.
 */
import { withOctocodeRender } from '../branding/renderers.js';
import type { ToolDefinition } from '../types.js';
import { PLAN_USAGE_GUIDANCE } from '@octocodeai/agent-contracts/prompts';
import { QueryBatchError } from './query-envelope.js';
import { ToolResultError } from './tool-result-error.js';

// ─── Registration helper ─────────────────────────────────────────────────────

export const DIRECT_TOOL_DESCRIPTIONS: Readonly<Record<string, string>> = Object.freeze({
  file: 'Mutate files with guarded edit, write, or delete. A local fix uses edit; write replaces the whole file and can erase unrelated work. Preserve current bytes outside the intended change. Read existing files, choose the matching operation, and batch edits to each path in one query.',
  bash: 'Run builds, tests, packages, and bounded debugging; never for code search or file reads. Use file for edits and MCPTool for evidence. A command without timeout can hang; guards do not make arbitrary scripts safe. Keep effects authorized, set a timeout, and inspect the exit result. Git requires an explicit user request.',
  inspectMedia: 'Inspect local image, video, or audio content. Use metadata for dimensions; use pixels, frames, or waveforms for visual evidence. Metadata alone cannot establish appearance. Inspect only the needed view; use media when the task requires an output file.',
  media: 'Create images/PDFs or transform media. Use inspectMedia to examine an existing file; use media to produce an artifact. Rendering success does not prove visual correctness. Choose the smallest supported transform, protect existing output, and inspect the result.',
  runFfmpeg: 'Run ffmpeg/ffprobe argv for operations such as filter_complex, loudnorm, or VMAF. Use media for a standard trim or conversion. Raw arguments can overwrite files; path guards are not consent. Pass argv without a shell or binary name, choose an authorized destination, and check the result.',
  web: 'Browse the live web for external facts. query discovers pages; url reads one. A search snippet is a lead, not proof of the page contents. Use repository/MCP tools for code evidence. Fetch the relevant source and follow needed continuation pages before making a claim.',
  chromeDebug: 'Inspect or operate Chrome through CDP. One screenshot uses this tool; dependent browser phases may use agent profile:browser. url navigates before inspection and can disturb current state. Preserve state outside the authorized journey. Attach to the known target, then run the smallest necessary scheme.',
  agent: 'Delegate bounded researcher, planner, architect, implementer, browser, or custom work. Custom requires tools and systemPrompt. Use MCPTool for repository research. The parent integrates, verifies the handback, and releases the worker.',
  callTool: 'Reuse or maintain a dynamic function. A recurring calculation may fit; a one-off shell command does not. Creating duplicates adds maintenance without capability. Reuse first; on a miss, research alternatives and obtain creation approval. Pass a reason, grant only approved capabilities, and verify the result.',
  skill: 'Load an installed skill for a specialized workflow or manage a reusable dynamic skill. Routine edits need none. Use type:load for installed instructions and type:call for dynamic lifecycle; read required instructions before acting.',
  plan: `${PLAN_USAGE_GUIDANCE} Extra tracking adds noise. Use set for authorized work, propose for review; complete only after an observed check.`,
  localServer: 'Serve an inspected static artifact on 127.0.0.1. Mount its directory, not an entire home or repository: every file in a mount may be exposed. Keep the served scope minimal. Use serve for a URL, open:true only with user authorization, and unmount when finished.',
  askUser: 'Collect one missing choice that changes the next action. A material trade-off needs an answer; routine authorized work does not need confirmation. Redundant questions stall work, and cancellation grants no authority. Choose one input mode, ask once, and use the explicit outcome.',
  awareness: 'Attend once and communicate when a peer must act. A blocker merits a signal; routine edits need no record. Reuse the host briefing, describe unfamiliar commands once, and call only the needed feature.',
  MCPTool: 'Call MCP tools, resources, and prompts; server:"octocode" holds the code, GitHub, history, npm, and semantic research catalog. The system prompt lists every enabled tool description and complete input schema. Put actions in queries[] and input in queries[].arguments only; octocode tools nest queries[] there. Describe can return one selected schema again.',
});

/** One executable discovery recipe; workers inherit it through the MCP gateway. */
export const MCP_SCHEMA_DISCOVERY_EXAMPLE = '{"queries":[{"reasoning":"Read the selected tool schema","server":"octocode","action":"describe","tool":"<catalog-tool-name>"}]}';

export interface DirectToolContractStats {
  tools: number;
  descriptionChars: number;
  schemaChars: number;
  totalChars: number;
}

const directToolContracts = new WeakMap<Set<string>, Map<string, { descriptionChars: number; schemaChars: number }>>();

export function getDirectToolContractStats(registeredToolNames: Set<string>): DirectToolContractStats {
  const contracts = directToolContracts.get(registeredToolNames);
  if (!contracts) return { tools: 0, descriptionChars: 0, schemaChars: 0, totalChars: 0 };
  let descriptionChars = 0;
  let schemaChars = 0;
  for (const contract of contracts.values()) {
    descriptionChars += contract.descriptionChars;
    schemaChars += contract.schemaChars;
  }
  return {
    tools: contracts.size,
    descriptionChars,
    schemaChars,
    totalChars: descriptionChars + schemaChars,
  };
}

function prepareQueryEnvelope(
  toolName: string,
  args: unknown,
): unknown {
  if (!args || typeof args !== 'object' || Array.isArray(args)) return args;
  const input = args as Record<string, unknown>;
  if (!Array.isArray(input['queries'])) return args;
  return {
    ...input,
    queries: input['queries'].map((value) => {
      if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
      const query = value as Record<string, unknown>;
      const reasoning = typeof query['reasoning'] === 'string' ? query['reasoning'].trim() : '';
      return reasoning ? query : { ...query, reasoning: `${toolName} operation` };
    }),
  };
}

export function registerUniqueTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  toolDefinition: ToolDefinition,
): void {
  if (registeredToolNames.has(toolDefinition.name)) {
    throw new Error(
      `Octocode Pi extension tool name collision: ${toolDefinition.name}`,
    );
  }
  if (typeof pi.registerTool !== 'function') {
    throw new Error('Octocode Pi extension requires the host registerTool API');
  }
  const description = DIRECT_TOOL_DESCRIPTIONS[toolDefinition.name] ?? toolDefinition.description;
  // Shorten descriptions at their source. Rewriting a schema here can erase
  // constraints or corrupt literal data inside examples/defaults.
  const parameters = toolDefinition.parameters;
  pi.registerTool(withOctocodeRender({
    ...toolDefinition,
    description,
    parameters,
    prepareArguments: (args: unknown) => prepareQueryEnvelope(toolDefinition.name, args),
    async execute(id, args, signal, onUpdate, ctx) {
      try {
        const result = await toolDefinition.execute(id, args, signal, onUpdate, ctx);
        if (result.isError) throw new ToolResultError(result, ctx, id, toolDefinition.name);
        return result;
      }
      catch (error) {
        if (error instanceof QueryBatchError && error.completedCount > 0) throw error.withHostReceipt(ctx, id);
        throw error;
      }
    },
  }));

  registeredToolNames.add(toolDefinition.name);
  let contracts = directToolContracts.get(registeredToolNames);
  if (!contracts) {
    contracts = new Map();
    directToolContracts.set(registeredToolNames, contracts);
  }
  contracts.set(toolDefinition.name, {
    descriptionChars: description.length,
    schemaChars: JSON.stringify(parameters).length,
  });
}
