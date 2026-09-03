/**
 * registerUniqueTool — shared helper used by all extension tool registrations.
 *
 * Native Octocode research tools (GitHub, local, LSP, npm) are no longer registered
 * as individual Pi tools. They are served via the bundled octocode MCP server through
 * MCPTool. This removes 13 tool definitions from the Pi tool palette, cutting per-turn
 * token cost. Full MCP discovery runs at session_start via warmMcpCatalog() and
 * before_agent_start awaits it (mcpCatalogReady). By default the first system
 * prompt receives the compact <mcp_catalog_index>; OCTOCODE_COMPACT_MCP=0 opts
 * into exact descriptions and schemas for debugging.
 */
import { withOctocodeRender } from '../branding/renderers.js';
import type { ToolDefinition } from '../types.js';

// ─── Registration helper ─────────────────────────────────────────────────────

export const DIRECT_TOOL_DESCRIPTIONS: Readonly<Record<string, string>> = Object.freeze({
  file: 'Create, edit, or delete files through one guarded mutation boundary. edit uses stale/lost-update checks and diffs; write is atomic; delete rejects directories and rechecks before unlinking.',
  bash: 'Run builds, tests, Git, and mechanical shell tasks with guarded write targets and per-command reasoning. Prefer file for ordinary file mutations.',
  readMedia: 'Read local media into model context. image returns pixels; video returns metadata, a frame, or contact sheet; audio returns metadata, waveform, or spectrogram. Read-only—use media to create or transform files.',
  media: 'Create or transform media. Render image/PDF from SVG, HTML, Markdown, or images; make GIFs, trim clips, extract audio, or convert formats. Writes are path-guarded; use readMedia for inspection.',
  runFfmpeg: 'Run advanced ffmpeg or ffprobe argv directly with workspace path guards, timeout, cancellation, and progress. Prefer readMedia and media for standard inspection and transforms.',
  web: 'Fetch a URL or search the current web for documentation, releases, errors, and other information outside the repository. Prefer repository/MCP tools for code evidence.',
  chromeDebug: 'Inspect or automate a live Chrome page through CDP: DOM, console, network, screenshots, performance, storage, security, coverage, or raw Domain.method calls. Use agent profile:browser for multi-turn browser work.',
  agent: 'Spawn researcher, planner, architect, browser, or custom workers; then inspect, wait, message, steer, abort, or kill them. Spawn first and use the returned agentId in later lifecycle calls.',
  callTool: 'Reuse a verified dynamic tool, or propose/create/fix/delete one after approval. Use only for small reusable deterministic capabilities—not trivial shell one-liners or multi-step workflows.',
  skill: 'Load an installed Agent Skill, or list/manage reusable dynamic workflow skills. Use type:load for installed skills and type:call for dynamic workflow lifecycle.',
  plan: 'Maintain a visible compaction-safe checklist. Use for multi-step/risky/shared work; skip obvious one-step tasks. Consequential RFCs require review then Start; shared completion requires an observed check receipt.',
  localServer: 'Serve inspected local static artifacts on a shared 127.0.0.1 server. Use for HTML plans/designs/reports; ask before opening a browser.',
  askUser: 'Ask one genuine decision question using options, multi-select, free text, or fields. Mark the safe default recommended and include concise trade-offs; ordinary conversation does not need this tool.',
  memory: 'Recall, record, review, suggest, or forget durable Awareness learning. Treat recall as a lead; record only verified reusable lessons, never secrets, routine status, or facts owned by code/docs.',
  lock: 'Acquire, wait for, or release an exceptional exclusive file lock. Ordinary mergeable edits rely on automatic peer checks and do not need a lock.',
  message: 'Send or read small cross-agent coordination messages for overlap, blockers, questions, or decisions. Do not use as routine status ceremony.',
  MCPTool: 'Call automatically discovered MCP tools with internally validated schemas, inspect one tool, or manage stdio and Streamable HTTP servers. Use server:octocode for repository research.',
});

export const SCHEMA_DESCRIPTION_MAX_CHARS = 180;

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

function compactSchemaValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(compactSchemaValue);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value as Record<string, unknown>).map(([key, child]) => {
    if (key !== 'description' || typeof child !== 'string') {
      return [key, compactSchemaValue(child)];
    }
    const normalized = child.replace(/\s+/g, ' ').trim();
    if (normalized.length <= SCHEMA_DESCRIPTION_MAX_CHARS) return [key, normalized];
    const prefix = normalized.slice(0, SCHEMA_DESCRIPTION_MAX_CHARS - 1);
    const boundary = prefix.lastIndexOf(' ');
    return [key, `${prefix.slice(0, boundary > 80 ? boundary : prefix.length)}…`];
  }));
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
  const parameters = compactSchemaValue(toolDefinition.parameters) as ToolDefinition['parameters'];
  pi.registerTool(withOctocodeRender({
    ...toolDefinition,
    description,
    parameters,
    prepareArguments: (args: unknown) => prepareQueryEnvelope(toolDefinition.name, args),
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
