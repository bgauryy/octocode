/**
 * spawnSubagent — typed subagent spawning.
 *
 * Unlike the generic spawnAgent, this tool:
 *   - Has a closed enum of registered subagent types (type-safe, discoverable)
 *   - Pre-loads the subagent's SYSTEM_PROMPT.md from dist/subagents/<name>/
 *   - Enforces the correct tool allowlist and resource mode per subagent
 *   - Loads every bundled Octocode skill for Octocode specialist subagents
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
  isSubagentProcess,
  type SpawnAgentParams,
} from './agent-tools.js';
import {
  SUBAGENT_REGISTRY,
  SUBAGENT_NAMES,
  loadSystemPrompt,
  type SubagentConfig,
  type SubagentName,
} from '../subagents.js';
import { getRandomAgentName } from '../agentNames.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;
const CHROME_DISABLED_ENV = 'OCTOCODE_CHROME_DEBUG';

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
  const codename = getRandomAgentName();
  let slug = '';
  if (params.url) {
    try {
      slug = ` · ${new URL(params.url).hostname.replace(/^www\./, '')}`;
    } catch {
      // ignore
    }
  }
  return `${config.label} · ${codename}${slug}`;
}

function isChromeDebugEnabled(): boolean {
  return process.env[CHROME_DISABLED_ENV] !== '0';
}

function getAvailableSubagentNames(): SubagentName[] {
  if (isChromeDebugEnabled()) return [...SUBAGENT_NAMES];
  return SUBAGENT_NAMES.filter((name) => name !== 'browser-agent');
}

function unavailableSubagentMessage(agent: string, availableNames: SubagentName[]): string {
  if (agent === 'browser-agent' && !isChromeDebugEnabled()) {
    return `browser-agent is unavailable because ${CHROME_DISABLED_ENV}=0 disables chromeDebug. Available: ${availableNames.join(', ')}`;
  }
  return `Unknown subagent: "${agent}". Available: ${availableNames.join(', ')}`;
}

// ─── Registration ─────────────────────────────────────────────────────────────

export function registerSpawnSubagentTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  _notify?: (ctx: PiContext | undefined, message: string, level?: string) => void,
): void {
  // Workers cannot spawn workers — never register this tool inside a spawned worker process.
  if (isSubagentProcess()) return;
  const availableSubagentNames = getAvailableSubagentNames();
  const availableSubagentSet = new Set<string>(availableSubagentNames);
  const availableSubagents = availableSubagentNames.map((name) => {
    const config = SUBAGENT_REGISTRY[name];
    return `  ${name} — ${config.description} Tools: ${config.tools.join(', ')}.`;
  }).join('\n');
  const skillGuideline = isChromeDebugEnabled()
    ? 'Every typed subagent loads all bundled Octocode skills; browser-agent also loads its browser-agent skill.'
    : 'Every available typed subagent loads all bundled Octocode skills; browser-agent is unavailable while Chrome debug is disabled.';

  registerFn(pi, registeredToolNames, {
    name: 'spawnSubagent',
    label: 'Spawn Subagent',
    description: [
      'Spawn a typed, pre-configured Pi subagent with the right tools, system prompt, resource mode, and all bundled Octocode skills.',
      'Use spawnAgent instead when you need a clean arbitrary worker with only the tools/skills you explicitly provide.',
      'Returns an agentId — use AgentMessage to coordinate (wait, send, steer, status, kill).',
      '',
      'Available subagents:',
      availableSubagents,
      '',
      'After spawning:',
      '  AgentMessage({action:"wait",   agentId, timeoutMs:60000})      — block until done',
      '  AgentMessage({action:"status", agentId})                       — poll without blocking',
      '  AgentMessage({action:"send",   agentId, message:"next task"})  — send follow-up',
      '  AgentMessage({action:"steer",  agentId, message:"new focus"})  — interrupt current turn',
      '  AgentMessage({action:"kill",   agentId, remove:true})          — terminate when done',
    ].join('\n'),

    promptSnippet: 'Spawn a typed Octocode specialist subagent with pre-configured tools, system prompt, and all Octocode skills',
    promptGuidelines: [
      `Use spawnSubagent for typed Octocode specialists: ${availableSubagentNames.join(', ')}.`,
      skillGuideline,
      'Use spawnAgent for clean arbitrary workers. spawnAgent defaults to lean/no-skills and only uses tools/skills you pass.',
      'Use `pi -ne --list-models [search]` as the source of truth for the user-configured model table; do not read hardcoded config paths.',
      'Pass model for each typed subagent: fastest capable configured model for small tasks, balanced coding/reasoning model for medium tasks, strongest configured model for large/high-risk work.',
      'Always follow with AgentMessage(wait) to collect results; use AgentMessage(send) for follow-up instructions.',
      'Typed subagents emit structured prefixed lines such as [FINDING], [EVIDENCE], [ACTION], [PLAN], [BLOCKED], and [DONE] — parse these for synthesis.',
      'Kill the agent with AgentMessage(kill, remove:true) when done to free resources.',
    ],

    parameters: Type.Object({
      agent: Type.Unsafe({
        type: 'string',
        enum: availableSubagentNames,
        description: `Subagent type to spawn. Available: ${availableSubagentNames.join(', ')}.`,
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
        Type.String({ description: 'Model override from `pi -ne --list-models [search]`. Defaults to subagent default. Choose from the live user-configured table; `--models` only sets model-cycling scope.' }),
      ),
      thinking: Type.Optional(
        Type.String({ description: 'Thinking level: off|minimal|low|medium|high|xhigh. Defaults to subagent default.' }),
      ),
      // browser-agent specific params (ignored by other subagents)
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
      const config: SubagentConfig | undefined = SUBAGENT_REGISTRY[params.agent];

      if (!config || !availableSubagentSet.has(params.agent)) {
        throw new Error(unavailableSubagentMessage(String(params.agent ?? ''), availableSubagentNames));
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
        model: params.model ?? config.model,
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
        `[SPAWNED] skills: ${(config.skills ?? []).map((skillPath) => skillPath.split(/[\\/]/).at(-1)).join(', ')}`,
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
