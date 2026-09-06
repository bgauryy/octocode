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
  file: 'Create, edit, or delete files through one guarded mutation boundary. Prefer file over bash for mutations. edit uses stale/lost-update checks and diffs; write is atomic; delete rejects directories and rechecks before unlinking.',
  bash: 'Run builds, tests, Git, and mechanical shell tasks with guarded write targets and per-command reasoning. Prefer file for ordinary file mutations; never for code search or file reads.',
  readMedia: 'Read local media. image->pixels; video->metadata/frame/contactSheet; audio->metadata/waveform/spectrogram. Read-only—use media to create or transform.',
  media: 'Create or transform media. Render image/PDF from SVG, HTML, Markdown, or images; make GIFs, trim clips, extract audio, or convert formats. Writes are path-guarded; use readMedia for inspection.',
  runFfmpeg: 'Run advanced ffmpeg or ffprobe argv directly with workspace path guards, timeout, cancellation, and progress. Prefer readMedia and media for standard operations. Use for filter_complex, loudnorm, or VMAF.',
  web: 'Fetch a URL or search the web for docs, releases, errors, and info outside the repository. Prefer repository/MCP tools for code evidence.',
  chromeDebug: 'Inspect/automate Chrome via CDP: DOM, console, network, screenshots, performance, storage, security, coverage, or raw Domain.method calls. Use agent profile:browser for multi-turn browser work.',
  agent: 'Spawn/manage researcher, planner, architect, browser, or custom workers. Spawn first; use agentId later. Researcher/planner/architect use MCPTool for repository research; architect uses bash only for tests/debugging.',
  callTool: 'Reuse a verified dynamic tool, or propose/create/fix/delete one after approval. Research existing tools first. Use only for small reusable deterministic capabilities—not trivial shell one-liners or multi-step workflows.',
  skill: 'Load an installed Agent Skill, or list/manage reusable dynamic workflow skills. Load a matching skill BEFORE acting. type:load for installed skills; type:call for dynamic lifecycle.',
  plan: 'Maintain a visible compaction-safe checklist. Use for multi-step/risky/shared work; skip obvious one-step tasks. Consequential RFCs need review then Start; shared completion needs a check receipt.',
  localServer: 'Serve inspected local static artifacts on a shared 127.0.0.1 server. Use for HTML plans/designs/reports; ask before opening a browser. Unmount when done.',
  askUser: 'Ask one genuine decision question using options, multi-select, free text, or fields. Mark the safe default recommended; ordinary conversation does not need this tool.',
  memory: 'Recall, record, review, suggest, or forget durable Awareness learning. Record only verified reusable lessons, never secrets, routine status, or facts owned by code/docs.',
  lock: 'Acquire, wait for, or release an exceptional file lock. Ordinary mergeable edits rely on automatic peer checks and do not need a lock.',
  message: 'Send or read small cross-agent coordination messages for overlap, blockers, or decisions. Do not use as routine status ceremony.',
  MCPTool: 'Use automatically discovered MCP tools; prefer server:"octocode" over bash for code search/file reads. Describe unfamiliar tools before their first call. Supports stdio and Streamable HTTP.',
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
