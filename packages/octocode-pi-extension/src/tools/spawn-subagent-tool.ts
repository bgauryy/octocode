/**
 * spawnSubagent — typed subagent spawning.
 *
 * Unlike the generic spawnAgent, this tool:
 *   - Has a closed enum of registered subagent types (type-safe, discoverable)
 *   - Pre-loads the subagent's SYSTEM_PROMPT.md from dist/subagents/<name>/
 *   - Enforces the correct tool allowlist and resource mode per subagent
 *   - Loads every bundled Octocode skill for Octocode specialist subagents
 *   - Returns agentId for AgentMessage (same agents Map as spawnAgent)
 *
 * Browser work uses the dedicated `browserAgent` tool (→ spawnAgent), not this
 * tool — so `browser-agent` is intentionally NOT a spawnSubagent type.
 *
 * Main agent workflow:
 *   1. spawnSubagent({agent:"researcher", task:"gather evidence on X"})
 *      → { agentId: "abc123", usage: "AgentMessage({action:\"wait\", agentId:\"abc123\"})" }
 *   2. AgentMessage({action:"wait", agentId:"abc123", timeoutMs:60000})
 *   3. AgentMessage({action:"send", agentId:"abc123", message:"now check the callers"})
 *   4. AgentMessage({action:"kill", agentId:"abc123", remove:true})
 */

import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { CLI_GLYPH, CLI_STATUS_TEXT, paint } from '../tui/cli-design.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import {
  spawnRpcAgent,
  isSubagentProcess,
  prepareSpawnAgentParams,
  type SpawnAgentParams,
} from './agent-tools.js';
import {
  SUBAGENT_REGISTRY,
  SUBAGENT_NAMES,
  loadSystemPrompt,
  resolveSubagentSkills,
  type SubagentConfig,
  type SubagentName,
} from '../subagents.js';
import { getRandomAgentName } from '../agentNames.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

// Browser work has a dedicated entry point (the `browserAgent` tool → spawnAgent),
// so `browser-agent` is excluded from the spawnSubagent type list to keep a single
// browser surface. The registry still defines it for the browserAgent tool + build.
const SPAWNABLE_SUBAGENT_NAMES = SUBAGENT_NAMES.filter((name) => name !== 'browser-agent');

// ─── Params per subagent type ─────────────────────────────────────────────────

interface SpawnSubagentParams {
  agent: SubagentName;
  task: string;
  context?: string;
  name?: string;
  cwd?: string;
  model?: string;
  // Pi provider name. When the model lives on a custom provider (e.g. a user-defined
  // provider whose model id collides with a builtin namespace like `claude-*`), passing
  // the provider explicitly disambiguates `--model` resolution. Without it, pi may match
  // the model id against a builtin provider and fail with "No API key found".
  provider?: string;
  thinking?: string;
  isolation?: SpawnAgentParams['isolation'];
  includeUncommitted?: boolean;
}

function buildTaskWithContext(params: SpawnSubagentParams): string {
  const lines: string[] = [];

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
  return `${config.label} · ${codename}`;
}

function unavailableSubagentMessage(agent: string, availableNames: SubagentName[]): string {
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
  const availableSubagentNames = SPAWNABLE_SUBAGENT_NAMES;
  const availableSubagentSet = new Set<string>(availableSubagentNames);
  const availableSubagents = availableSubagentNames.map((name) => {
    const config = SUBAGENT_REGISTRY[name];
    return `  ${name} — ${config.description} Tools: ${config.tools.join(', ')}.`;
  }).join('\n');
  const skillGuideline = 'Every typed subagent loads any Octocode skills already installed. For browser/Chrome DevTools work use the dedicated `browserAgent` tool instead of spawnSubagent.';

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
      '  AgentMessage({action:"wait",   agentId, timeoutMs:60000})      — wait for the current turn',
      '  AgentMessage({action:"status", agentId})                       — poll without blocking',
      '  AgentMessage({action:"send",   agentId, message:"next task"})  — send follow-up',
      '  AgentMessage({action:"steer",  agentId, message:"new focus"})  — redirect before the next model step',
      '  AgentMessage({action:"kill",   agentId, remove:true})          — terminate when done',
    ].join('\n'),

    promptSnippet: 'Spawn a typed Octocode specialist subagent with pre-configured tools, system prompt, and all Octocode skills',
    promptGuidelines: [
      `Use spawnSubagent for typed Octocode specialists: ${availableSubagentNames.join(', ')}.`,
      skillGuideline,
      'Use spawnAgent for clean arbitrary workers. spawnAgent defaults to lean/no-skills and only uses tools/skills you pass.',
      'Before spawning, break the request into explicit subtasks and delegate only one independent, bounded subtask per typed specialist.',
      'Structure the task as a labeled packet — lines starting with "Goal:", "Context:", "Scope:", "Ownership:", "Acceptance:", "Return:" (any of "-"/"—"/":" as separator, headings/bullets OK). Missing labels surface as a [POLICY] warning on the spawn response, not silently.',
      'Model routing (which configured model to pass, `pi -ne --list-models`) is defined once in the agents policy — follow it there rather than re-deriving it here.',
      'Use AgentMessage(wait) to collect the current turn; treat [DONE] as phase completion and check /octocode-agents or the below-editor ledger plus the delegated acceptance criteria before declaring the objective complete.',
      'Typed subagent packets include a durable handback file under .octocode/tmp/agents/<agentId>/handback.md and typed subagents have the write tool; require long or important findings to be written there and reported with [ARTIFACT] before terminal output.',
      'Use AgentMessage(abort) to gracefully interrupt the active turn — the subagent stays alive for follow-up send/steer turns.',
      'Typed subagents emit structured prefixed lines such as [FINDING], [EVIDENCE], [ACTION], [PLAN], [BLOCKED], [ARTIFACT], and [DONE] — parse these for synthesis.',
      'Before killing/removing an important worker, inspect AgentMessage(status/wait, full:true) and any handback file it reports; then kill the agent with AgentMessage(kill, remove:true) to free resources.',
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
      provider: Type.Optional(
        Type.String({ description: 'Pi provider name for the model. REQUIRED when the model id collides with a builtin provider namespace (e.g. a custom provider offering `claude-*`); without it pi may resolve to the builtin provider and fail with "No API key found". Look up via `pi -ne --list-models [search]`.' }),
      ),
      thinking: Type.Optional(
        Type.String({ description: 'Thinking level: off|minimal|low|medium|high|xhigh. Defaults to subagent default.' }),
      ),
      isolation: Type.Optional(Type.Unsafe({ type: 'string', enum: ['shared', 'worktree'], description: 'Worker filesystem isolation. "shared" (default) uses the current cwd; "worktree" asks before creating an isolated git worktree.' })),
      includeUncommitted: Type.Optional(Type.Boolean({ description: 'With isolation:"worktree", apply a tracked-change snapshot from the parent tree. Untracked files are not included.' })),
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
      const skills = resolveSubagentSkills(config);
      const spawnParams: SpawnAgentParams = {
        task: buildTaskWithContext(params),
        name: buildAgentName(params),
        cwd: params.cwd,
        tools: [...config.tools],
        skills,
        resourceMode: config.resourceMode,
        systemPrompt,
        thinking: params.thinking ?? config.thinking,
        model: params.model ?? config.model,
        provider: params.provider ?? config.provider ?? ctx?.model?.provider,
        noSession: true,
        isolation: params.isolation,
        includeUncommitted: params.includeUncommitted,
      };

      // Spawn via the same internal function as spawnAgent → same agents Map → AgentMessage works
      const approvedParams = await prepareSpawnAgentParams(spawnParams, ctx);
      const record = spawnRpcAgent(approvedParams, ctx);

      const agentId = record.id;
      const usage = [
        `AgentMessage({action:"wait",   agentId:"${agentId}", timeoutMs:60000})`,
        `AgentMessage({action:"send",   agentId:"${agentId}", message:"<follow-up>"})`,
        `AgentMessage({action:"status", agentId:"${agentId}"})`,
        `AgentMessage({action:"abort",  agentId:"${agentId}"})`,
        `AgentMessage({action:"kill",   agentId:"${agentId}", remove:true})`,
      ].join('\n');

      // Surface spawn-time policy warnings (packet gaps, fan-out, recursive-tool
      // stripping, provider guidance) immediately — the caller should not have to
      // spend a round-trip AgentMessage(wait) just to discover the delegation was
      // under-specified. spawnAgent's own execute already does this via
      // renderSingleAgentResult; spawnSubagent's custom [SPAWNED] output previously
      // dropped policyWarnings entirely.
      const policyLines = record.policyWarnings.length > 0
        ? ['', '[POLICY]', ...record.policyWarnings.map((warning) => `  ${warning}`)]
        : [];

      const output = [
        `[SPAWNED] ${config.label} · agentId: ${agentId}`,
        `[SPAWNED] name: ${record.name}`,
        `[SPAWNED] tools: ${config.tools.join(', ')}`,
        `[SPAWNED] skills: ${skills.map((skillPath) => skillPath.split(/[\/]/).at(-1)).join(', ')}`,
        `[SPAWNED] resourceMode: ${config.resourceMode}`,
        `[SPAWNED] task: ${params.task.slice(0, 120)}${params.task.length > 120 ? '…' : ''}`,
        ...policyLines,
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
      // Pi invokes renderCall with PARTIAL args during argument streaming; every
      // field may still be absent.
      const p = (rawParams ?? {}) as Partial<SpawnSubagentParams>;
      const config = p.agent ? SUBAGENT_REGISTRY[p.agent as SubagentName] : undefined;
      const label = config?.label ?? p.agent ?? '…';
      const task = typeof p.task === 'string' ? p.task : '';
      const raw = `spawnSubagent(${label}) "${task.slice(0, 45)}${task.length > 45 ? '…' : ''}"`;
      return makeRenderer((w) => [truncateToWidth(raw, w)]);
    },

    renderResult(result: unknown, opts: { isPartial?: boolean } | undefined, theme?: PiTheme) {
      const r = result as { content?: Array<{ text?: string }>; isError?: boolean };
      const text = r?.content?.[0]?.text ?? '';
      // In-flight (streaming/approval pending): show a running row, not a fake
      // "spawned" claim.
      if (opts?.isPartial) {
        const prog = paint(theme, 'brand', `${CLI_STATUS_TEXT.running} spawnSubagent`);
        return makeRenderer((w) => [truncateToWidth(prog, w)]);
      }
      // Failed spawn (unknown agent, declined worktree approval, RPC error):
      // surface the error line with the shared failure glyph instead of
      // pretending the agent spawned.
      if (r?.isError) {
        const firstLine = text.split('\n').find(Boolean) ?? 'spawn failed';
        const raw = paint(theme, 'error', `${CLI_GLYPH.error} spawnSubagent: ${firstLine}`);
        return makeRenderer((w) => [truncateToWidth(raw, w)]);
      }
      const lines = text.split('\n');
      const agentLine = lines.find((l) => l.startsWith('[SPAWNED]')) ?? '';
      const hasUsage = lines.some((l) => l.startsWith('AgentMessage({action:"wait"'));
      const raw = agentLine
        ? `${paint(theme, 'success', agentLine)}${hasUsage ? paint(theme, 'dim', ' · use AgentMessage wait/status; see /octocode-agents') : ''}`
        : 'spawnSubagent: spawned';
      return makeRenderer((w) => [truncateToWidth(raw, w)]);
    },
  });
}
