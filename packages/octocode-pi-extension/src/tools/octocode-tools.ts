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
import { QueryBatchError } from './query-batch-error.js';
import { ToolResultError } from './tool-result-error.js';

// ─── Registration helper ─────────────────────────────────────────────────────

export const DIRECT_TOOL_DESCRIPTIONS: Readonly<Record<string, string>> = Object.freeze({
  file: 'Edit, write, or delete files with guards. Edit targets bytes; write replaces all. Batch edits per path.',
  bash: 'Run builds, tests, or packages. Set timeout. background:true returns a jobId. Manage with action:status|output|kill|list.',
  inspectMedia: 'Inspect image, video, or audio — metadata, pixels, frames, or waveforms.',
  media: 'Create images/PDFs or transform media. Inspect output after creation.',
  runFfmpeg: 'Run ffmpeg/ffprobe argv for filter_complex, loudnorm, VMAF, etc. Pass argv without shell or binary name.',
  web: 'Browse the live web. query finds pages; url reads one. Fetch source before making claims.',
  chromeDebug: 'Inspect/operate Chrome via CDP. Attach to known target; run minimal scheme.',
  agent: 'Delegate bounded workers for parallel or specialist work. Verify handbacks. Custom requires tools+systemPrompt.',
  callTool: 'Reuse or maintain a dynamic function. Reuse first; creation requires approval.',
  skill: 'Load an installed skill or manage dynamic skills. type:load for SKILL.md; type:call for lifecycle.',
  plan: `${PLAN_USAGE_GUIDANCE}`,
  localServer: 'Serve a static artifact on 127.0.0.1. Mount minimal scope; unmount when done.',
  askUser: 'Collect one missing choice that changes the next action. Ask once.',
  awareness: 'Shared coordination state. Start with context.orient; batch reads; one mutation per call.',
  MCPTool: 'Call MCP tools, resources, and prompts. server:"octocode" for code/GitHub/npm research. Batch Octocode calls inside arguments.queries[]. Use action:describe only for missing/stale schemas.',
});

/** One executable discovery recipe; workers inherit it through the MCP gateway. */
export const MCP_SCHEMA_DISCOVERY_EXAMPLE = '{"queries":[{"reasoning":"Read the selected tool schema","server":"octocode","action":"describe","tool":"<catalog-tool-name>"}]}';
/** One executable Octocode call recipe showing the outer and target query boundaries. */
export const OCTOCODE_MCP_CALL_EXAMPLE = '{"queries":[{"reasoning":"Read the known file","action":"call","server":"octocode","tool":"localFetch","arguments":{"queries":[{"path":"/ABS/repo/README.md","fullContent":true}]}}]}';

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
    // Pi flattens guidelines from active tools into one unlabelled section.
    promptGuidelines: toolDefinition.promptGuidelines?.map(guideline => `${toolDefinition.name}: ${guideline}`),
    prepareArguments: (args: unknown) => prepareQueryEnvelope(
      toolDefinition.name,
      toolDefinition.prepareArguments ? toolDefinition.prepareArguments(args) : args,
    ),
    async execute(id, args, signal, onUpdate, ctx) {
      try {
        const result = await toolDefinition.execute(id, args, signal, onUpdate, ctx);
        if (result.isError) throw new ToolResultError(result, toolDefinition.name);
        return result;
      }
      catch (error) {
        if (error instanceof QueryBatchError) throw error.withHostReceipt();
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
