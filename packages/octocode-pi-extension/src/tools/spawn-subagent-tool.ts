/**
 * spawnSubagent — typed subagent spawning.
 *
 * Unlike the generic spawnAgent, this tool:
 *   - Has a closed enum of registered subagent types (type-safe, discoverable)
 *   - Pre-loads the subagent's SYSTEM_PROMPT.md from dist/subagents/<name>/
 *   - Enforces the correct tool allowlist and resource mode per subagent
 *   - Passes subagent-specific params (url, port) as structured context in the task
 *   - Returns agentId for AgentMessage (same agents Map as spawnAgent)
 *
 * Main agent workflow:
 *   1. spawnSubagent({agent:"browser-agent", task:"audit cookies on example.com", url:"https://example.com"})
 *      → { agentId: "abc123", usage: "AgentMessage({action:\"wait\", agentId:\"abc123\"})" }
 *   2. AgentMessage({action:"wait", agentId:"abc123", timeoutMs:60000})
 *   3. AgentMessage({action:"send", agentId:"abc123", message:"now check service workers"})
 *   4. AgentMessage({action:"kill", agentId:"abc123", remove:true})
 */

import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import {
  spawnRpcAgent,
  type SpawnAgentParams,
} from './agent-tools.js';
import {
  SUBAGENT_REGISTRY,
  SUBAGENT_NAMES,
  loadSystemPrompt,
  type SubagentName,
} from '../subagents.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

// ─── Params per subagent type ─────────────────────────────────────────────────

interface SpawnSubagentParams {
  agent: SubagentName;
  task: string;
  context?: string;
  name?: string;
  cwd?: string;
  model?: string;
  thinking?: string;
  // browser-agent extras (injected into task context block)
  url?: string;
  port?: number;
  launch?: boolean;
  headless?: boolean;
}

function buildTaskWithContext(params: SpawnSubagentParams): string {
  const agent = params.agent;
  const lines: string[] = [];

  // Browser-agent: inject session params at top so the subagent has them from turn 1
  if (agent === 'browser-agent') {
    lines.push('## Browser Session');
    if (params.url) lines.push(`Target URL: ${params.url}`);
    lines.push(`Chrome port: ${params.port ?? 9222}`);
    if (params.launch) lines.push(`Launch Chrome: true (start Chrome if not running)`);
    if (params.headless === false) lines.push(`Headless: false (visible Chrome)`);
    lines.push('');
  }

  if (params.context) {
    lines.push('## Context');
    lines.push(params.context.trim());
    lines.push('');
  }

  lines.push('## Task');
  lines.push(params.task.trim());

  return lines.join('\n');
}

function buildAgentName(params: SpawnSubagentParams): string {
  if (params.name) return params.name;
  const config = SUBAGENT_REGISTRY[params.agent];
  const slug = params.url
    ? new URL(params.url).hostname.replace(/^www\./, '')
    : 'session';
  return `${config.label} · ${slug}`;
}

// ─── Registration ─────────────────────────────────────────────────────────────

export function registerSpawnSubagentTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  _notify?: (ctx: PiContext | undefined, message: string, level?: string) => void,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'spawnSubagent',
    label: 'Spawn Subagent',
    description: [
      'Spawn a typed, pre-configured Pi subagent with the right tools, system prompt, and resource mode.',
      'Returns an agentId — use AgentMessage to coordinate (wait, send, steer, status, kill).',
      '',
      'Available subagents:',
      '  browser-agent — Chrome DevTools specialist. Tools: chromeDebug, web, localGetFileContent,',
      '                  localSearchCode, localViewStructure. Emits [FINDING]/[ACTION]/[DONE] protocol.',
      '                  Use for: security audits, network analysis, DOM inspection, coverage,',
      '                  workers, service workers, emulation, automation — any multi-turn browser work.',
      '',
      'After spawning:',
      '  AgentMessage({action:"wait",   agentId, timeoutMs:60000})      — block until done',
      '  AgentMessage({action:"status", agentId})                       — poll without blocking',
      '  AgentMessage({action:"send",   agentId, message:"next task"})  — send follow-up',
      '  AgentMessage({action:"steer",  agentId, message:"new focus"})  — interrupt current turn',
      '  AgentMessage({action:"kill",   agentId, remove:true})          — terminate when done',
    ].join('\n'),

    promptSnippet: 'Spawn a typed browser/specialist subagent with pre-configured tools and system prompt',
    promptGuidelines: [
      'Use spawnSubagent instead of spawnAgent when the task needs a specialised subagent (browser-agent for any Chrome DevTools work).',
      'browser-agent has chromeDebug + web + local search — perfect for multi-turn browser sessions.',
      'Always follow with AgentMessage(wait) to collect results; use AgentMessage(send) for follow-up instructions.',
      'browser-agent emits structured [FINDING]/[ACTION]/[METRIC]/[DONE] lines — parse these for findings.',
      'Kill the agent with AgentMessage(kill, remove:true) when done to free resources.',
    ],

    parameters: Type.Object({
      agent: Type.Unsafe({
        type: 'string',
        enum: SUBAGENT_NAMES,
        description: 'Subagent type to spawn. Currently: "browser-agent".',
      }),
      task: Type.String({
        description:
          'What the subagent should do. Be specific: include URLs, what to look for, what to emit. ' +
          'The subagent stays alive — you can send follow-ups via AgentMessage.',
      }),
      context: Type.Optional(
        Type.String({ description: 'Additional context prepended to the task (background info, prior findings).' }),
      ),
      name: Type.Optional(
        Type.String({ description: 'Human label for AgentMessage list output. Auto-generated if omitted.' }),
      ),
      cwd: Type.Optional(
        Type.String({ description: 'Working directory for the subagent process. Defaults to current cwd.' }),
      ),
      model: Type.Optional(
        Type.String({ description: 'Model override, e.g. "sonnet:high". Defaults to subagent default.' }),
      ),
      thinking: Type.Optional(
        Type.String({ description: 'Thinking level: off|minimal|low|medium|high|xhigh. Defaults to subagent default.' }),
      ),
      // browser-agent specific params
      url: Type.Optional(
        Type.String({ description: '(browser-agent) Target URL. Injected into task context.' }),
      ),
      port: Type.Optional(
        Type.Integer({ description: '(browser-agent) Chrome remote debug port. Default 9222.' }),
      ),
      launch: Type.Optional(
        Type.Boolean({ description: '(browser-agent) Launch Chrome if not running. Default false.' }),
      ),
      headless: Type.Optional(
        Type.Boolean({ description: '(browser-agent) Headless Chrome. Default true.' }),
      ),
    }),

    async execute(
      _toolCallId: string,
      rawParams: Record<string, unknown>,
      _signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: PiContext,
    ) {
      const params = rawParams as unknown as SpawnSubagentParams;
      const config = SUBAGENT_REGISTRY[params.agent];

      if (!config) {
        throw new Error(
          `Unknown subagent: "${params.agent}". Available: ${SUBAGENT_NAMES.join(', ')}`,
        );
      }

      // Load system prompt from dist/subagents/<name>/SYSTEM_PROMPT.md
      const systemPrompt = loadSystemPrompt(config);

      // Build spawn params
      const spawnParams: SpawnAgentParams = {
        task: buildTaskWithContext(params),
        name: buildAgentName(params),
        cwd: params.cwd,
        tools: [...config.tools],
        skills: config.skills,
        resourceMode: config.resourceMode,
        systemPrompt,
        thinking: params.thinking ?? config.thinking,
        model: params.model,  // config has no model default — use param only
        noSession: true,
      };

      // Spawn via the same internal function as spawnAgent → same agents Map → AgentMessage works
      const record = spawnRpcAgent(spawnParams, ctx);

      const agentId = record.id;
      const usage = [
        `AgentMessage({action:"wait",   agentId:"${agentId}", timeoutMs:60000})`,
        `AgentMessage({action:"send",   agentId:"${agentId}", message:"<follow-up>"})`,
        `AgentMessage({action:"status", agentId:"${agentId}"})`,
        `AgentMessage({action:"kill",   agentId:"${agentId}", remove:true})`,
      ].join('\n');

      const output = [
        `[SPAWNED] ${config.label} · agentId: ${agentId}`,
        `[SPAWNED] name: ${record.name}`,
        `[SPAWNED] tools: ${config.tools.join(', ')}`,
        `[SPAWNED] resourceMode: ${config.resourceMode}`,
        `[SPAWNED] task: ${params.task.slice(0, 120)}${params.task.length > 120 ? '…' : ''}`,
        '',
        '[USAGE]',
        usage,
      ].join('\n');

      return {
        content: [{ type: 'text', text: output }],
        agentId,
      } as unknown as ToolCallResult;
    },

    renderCall(rawParams: unknown) {
      const p = rawParams as SpawnSubagentParams;
      const config = SUBAGENT_REGISTRY[p.agent as SubagentName];
      const label = config?.label ?? p.agent;
      const url = p.url ? ` → ${p.url}` : '';
      const raw = `spawnSubagent(${label}${url}) "${p.task.slice(0, 45)}${p.task.length > 45 ? '…' : ''}"`;
      return makeRenderer((w) => [truncateToWidth(raw, w)]);
    },

    renderResult(result: unknown, _opts: unknown, theme?: PiTheme) {
      const r = result as { content?: Array<{ text?: string }> };
      const text = r?.content?.[0]?.text ?? '';
      const agentLine = text.split('\n').find((l) => l.startsWith('[SPAWNED]')) ?? '';
      const raw = (theme?.fg('success', agentLine) ?? agentLine) || 'spawnSubagent: spawned';
      return makeRenderer((w) => [truncateToWidth(raw, w)]);
    },
  });
}
