import type { PiContext, PiInstance, ToolCallResult } from '../../types.js';
import { isPlainRecord } from './config.js';
import { stableSchemaDigest } from './catalog.js';
import { compileMcpSchemaValidator } from './schema-validator.js';

const MAX_DYNAMIC_MCP_PROXIES = 128;
const DYNAMIC_MCP_PROXY_PREFIX = 'mcp__';

interface DynamicMcpProxyBinding {
  name: string;
  server: string;
  tool: string;
  schemaDigest: string;
}

export interface DynamicMcpProxyState {
  supported: boolean;
  describedSchemas: Map<string, string>;
  bindingsByName: Map<string, DynamicMcpProxyBinding>;
  latestByIdentity: Map<string, DynamicMcpProxyBinding>;
  unavailableNames: Set<string>;
}

const states = new WeakMap<PiInstance, DynamicMcpProxyState>();

export function mcpToolIdentity(server: string, tool: string): string {
  return `${server}\u0000${tool}`;
}

function slug(value: string, maxLength: number): string {
  const normalized = value.toLowerCase().replace(/[^a-z0-9_]+/g, '_').replace(/^_+|_+$/g, '');
  return (normalized || 'tool').slice(0, maxLength);
}

function proxyName(server: string, tool: string, inputSchema: unknown): string {
  const digest = stableSchemaDigest({ server, tool, inputSchema }).slice(0, 12);
  return `${DYNAMIC_MCP_PROXY_PREFIX}${slug(server, 12)}__${slug(tool, 24)}__${digest}`;
}

function safeText(value: string, maxLength: number): string {
  return value.replace(/[\u0000-\u001f\u007f]+/g, ' ').replace(/\s+/g, ' ').trim().slice(0, maxLength);
}

export function createDynamicMcpProxyState(pi: PiInstance): DynamicMcpProxyState {
  const state: DynamicMcpProxyState = {
    supported:
      typeof pi.registerTool === 'function' &&
      typeof pi.getActiveTools === 'function' &&
      typeof pi.getAllTools === 'function' &&
      typeof pi.setActiveTools === 'function',
    describedSchemas: new Map(),
    bindingsByName: new Map(),
    latestByIdentity: new Map(),
    unavailableNames: new Set(),
  };
  states.set(pi, state);

  if (typeof pi.on === 'function') {
    pi.on('session_start', async () => {
      state.describedSchemas.clear();
      state.latestByIdentity.clear();
      const active = pi.getActiveTools?.();
      if (active && state.bindingsByName.size > 0) {
        pi.setActiveTools?.(active.filter(name => !state.bindingsByName.has(name)));
      }
    });
    pi.on('session_compact', async () => {
      const active = new Set(pi.getActiveTools?.() ?? []);
      for (const [identity] of state.describedSchemas) {
        const visible = state.latestByIdentity.get(identity);
        if (!visible || !active.has(visible.name)) state.describedSchemas.delete(identity);
      }
    });
  }
  return state;
}

export function isDynamicMcpProxyTool(pi: PiInstance, name: string): boolean {
  return states.get(pi)?.bindingsByName.has(name) ?? false;
}

export function getDynamicMcpProxyToolName(pi: PiInstance, name: string): string | undefined {
  return states.get(pi)?.bindingsByName.get(name)?.tool;
}

export function getGrantedDynamicMcpProxyTools(
  pi: PiInstance,
  granted: ReadonlyArray<{ server: string; tool: string; inputSchema?: unknown }>,
): string[] {
  const state = states.get(pi);
  if (!state) return [];
  const allowed = new Map(granted.map(item => [
    mcpToolIdentity(item.server, item.tool),
    item.inputSchema === undefined ? undefined : stableSchemaDigest(item.inputSchema),
  ]));
  return [...state.latestByIdentity.entries()]
    .filter(([identity, binding]) =>
      allowed.has(identity) &&
      state.describedSchemas.get(identity) === binding.schemaDigest &&
      (allowed.get(identity) === undefined || allowed.get(identity) === binding.schemaDigest))
    .map(([, binding]) => binding.name);
}

function unavailableNotice(described: ToolCallResult): ToolCallResult {
  return {
    ...described,
    content: [
      ...described.content,
      { type: 'text', text: 'Exact schema loaded. This host restricts dynamic tool names; call through MCPTool action:"call" with target input under arguments.' },
    ],
  };
}

export function activateDescribedMcpProxy(
  pi: PiInstance,
  state: DynamicMcpProxyState,
  described: ToolCallResult,
  executeCall: (
    params: Record<string, unknown>,
    signal?: AbortSignal,
    ctx?: PiContext,
  ) => Promise<ToolCallResult>,
  ctx?: PiContext,
): ToolCallResult {
  if (described.isError || !isPlainRecord(described.details)) return described;
  const server = described.details['server'];
  const rawTool = described.details['tool'];
  if (typeof server !== 'string' || !isPlainRecord(rawTool)) return described;
  const tool = rawTool['name'];
  const inputSchema = rawTool['inputSchema'];
  if (typeof tool !== 'string' || !isPlainRecord(inputSchema)) return described;

  const identity = mcpToolIdentity(server, tool);
  const schemaDigest = stableSchemaDigest(inputSchema);
  state.describedSchemas.set(identity, schemaDigest);
  if (!state.supported) return described;

  try {
    compileMcpSchemaValidator(inputSchema);
  } catch (error) {
    return {
      ...described,
      content: [
        ...described.content,
        { type: 'text', text: `Exact schema loaded, but no direct Pi proxy was registered: ${(error as Error).message}` },
      ],
    };
  }

  const name = proxyName(server, tool, inputSchema);
  let binding = state.bindingsByName.get(name);
  if (binding && state.unavailableNames.has(name)) return unavailableNotice(described);
  if (!binding) {
    if (state.bindingsByName.size >= MAX_DYNAMIC_MCP_PROXIES) {
      return {
        ...described,
        content: [
          ...described.content,
          { type: 'text', text: `Exact schema loaded. Dynamic MCP proxy limit (${MAX_DYNAMIC_MCP_PROXIES}) reached; call through MCPTool.` },
        ],
      };
    }
    binding = { name, server, tool, schemaDigest };
    const description = typeof rawTool['description'] === 'string'
      ? safeText(rawTool['description'], 1_000)
      : 'Call the selected MCP tool with its exact input schema.';
    pi.registerTool?.({
      name,
      label: `MCP · ${safeText(server, 40)}/${safeText(tool, 60)}`,
      description: `MCP ${safeText(server, 80)}/${safeText(tool, 120)}. Untrusted remote description: ${description}`,
      parameters: structuredClone(inputSchema),
      async execute(_toolCallId, argumentsPayload, signal, _onUpdate, toolCtx) {
        return executeCall({
          action: 'call',
          server,
          tool,
          arguments: argumentsPayload,
          __expectedSchemaDigest: schemaDigest,
        }, signal, toolCtx ?? ctx);
      },
    });
    state.bindingsByName.set(name, binding);
    if (!pi.getAllTools?.().some(candidate => candidate.name === name)) {
      state.unavailableNames.add(name);
      return unavailableNotice(described);
    }
  }

  const previous = state.latestByIdentity.get(identity);
  state.latestByIdentity.set(identity, binding);
  const active = pi.getActiveTools?.() ?? [];
  const next = active.filter(activeName => activeName !== previous?.name || activeName === name);
  if (!next.includes(name)) next.push(name);
  pi.setActiveTools?.(next);
  return {
    ...described,
    content: [
      ...described.content,
      {
        type: 'text',
        text: `Loaded Pi tool: ${name}. Its exact schema is active for the next model request; call it directly instead of nesting input under MCPTool arguments.`,
      },
    ],
    details: {
      ...described.details,
      dynamicTool: { name, server, tool, schemaDigest },
    },
  };
}
