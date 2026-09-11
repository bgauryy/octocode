/**
 * tool.ts — Unified agent tool facade.
 *
 * Registers the single public `agent` tool that wraps spawn/inspect/wait/
 * message/steer/abort/kill operations. Schema, validation, rendering, and
 * query-batch dispatch only; all business logic lives in sibling modules.
 *
 * Imports plan-integration (triggers side-effect registration) and lifecycle
 * (owns executeSpawnQuery and executeAgentLifecycle).
 */

import type {
  ToolDefinition,
  ToolCallResult,
  PiContext,
  PiTheme,
} from '../../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from '../octocode-tools.js';
import {
  AGENT_PROFILES,
  AGENT_OPERATIONS,
  type AgentOperation,
  rejectCrossBatchReference,
} from './plan-integration.js';
import { executeSpawnQuery, executeAgentLifecycle } from './lifecycle.js';
import { isSubagentProcess } from './registry.js';
import {
  executeQueryBatch,
  toToolSchema,
  type QueryRecord,
} from '../query-envelope.js';
import { makeComponentRenderer } from '../render-helpers.js';
import { truncateToWidth } from '../../tui/width.js';
import { CLI_GLYPH } from '../../tui/cli-design.js';
import { paint } from '../../tui/palette.js';

import { z } from 'zod';
import { WorkerCapabilitySelectionSchema } from '@octocodeai/agent-contracts/capabilities';
type RegisterFn = typeof registerUniqueTool;

/** Register the single public agent tool. */
export function registerUnifiedAgentTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  // Workers cannot spawn workers — never register this tool inside a subagent process.
  if (isSubagentProcess()) return;

  // ── Discriminated operation schema ───────────────────────────────────────────
  const reasoning = z.string().min(1).max(400);
  const packetFields = {
    goal: z.string().min(1),
    context: z.string().min(1),
    scope: z.string().min(1),
    ownership: z.string().min(1),
    acceptance: z.string().min(1),
    returnShape: z.string().min(1),
    task: z.string().optional(),
    name: z.string().optional(),
    model: z.string().optional().describe('Model id from `pi -ne --list-models`.'),
    provider: z.string().optional(),
    thinking: z.enum(['off', 'minimal', 'low', 'medium', 'high', 'xhigh']).optional(),
    noSession: z.boolean().optional(),
    isolation: z.enum(['shared', 'worktree']).optional(),
    includeUncommitted: z.boolean().optional(),
    planStep: z.string().optional().describe('Plan task ID only; omit for standalone delegation.'),
    cohortId: z.string().min(1).max(80).optional(),
    capabilities: WorkerCapabilitySelectionSchema.optional(),
    snapshotRevision: z.string().min(1).optional(),
    evidence: z.string().max(16_000).optional(),
    instructions: z.string().max(16_000).optional(),
    skillResources: z.array(z.string().min(1).max(4096)).max(50).optional(),
  };
  const typedSpawn = z.strictObject({
    reasoning,
    type: z.enum(['spawn']),
    profile: z.enum(['researcher', 'planner', 'architect', 'implementer', 'reviewer']),
    ...packetFields,
  });
  const browserSpawn = z.strictObject({
    reasoning,
    type: z.enum(['spawn']),
    profile: z.enum(['browser']),
    ...packetFields,
    url: z.string().optional(),
    port: z.number().int().optional(),
    launch: z.boolean().optional(),
    headless: z.boolean().optional(),
    runNow: z.boolean().optional(),
    durationMs: z.number().int().optional(),
    workspaceCwd: z.string().optional(),
  });
  const customSpawn = z.strictObject({
    reasoning,
    type: z.enum(['spawn']),
    profile: z.enum(['custom']),
    ...packetFields,
    tools: z.array(z.string()),
    systemPrompt: z.string().min(1),
    resourceMode: z.enum(['lean', 'octocode', 'default']).optional(),
  });
  const inspect = z.strictObject({ reasoning, type: z.enum(['inspect']), agentId: z.string().optional(), full: z.boolean().optional() });
  const configure = z.strictObject({ reasoning, type: z.enum(['configure']), agentId: z.string().min(1), snapshotRevision: z.string().min(1), grantRevision: z.number().int().positive().optional(), capabilities: WorkerCapabilitySelectionSchema });
  const wait = z.strictObject({ reasoning, type: z.enum(['wait']), agentId: z.string().min(1), timeoutMs: z.number().int().optional(), remove: z.boolean().optional(), full: z.boolean().optional() });
  const message = z.strictObject({ reasoning, type: z.enum(['message']), agentId: z.string().min(1), message: z.string().min(1), delivery: z.enum(['send', 'followUp']).optional() });
  const steer = z.strictObject({ reasoning, type: z.enum(['steer']), agentId: z.string().min(1), message: z.string().min(1) });
  const abort = z.strictObject({ reasoning, type: z.enum(['abort']), agentId: z.string().min(1), full: z.boolean().optional() });
  const kill = z.strictObject({ reasoning, type: z.enum(['kill']), agentId: z.string().min(1), remove: z.boolean().optional(), full: z.boolean().optional() });
  const query = z.union([typedSpawn, browserSpawn, customSpawn, inspect, configure, wait, message, steer, abort, kill]);
  const parameters = toToolSchema(z.strictObject({
    queries: z.array(query).min(1).max(100).describe('Operations run one-by-one in source order.'),
    queryRunType: z.enum(['sequential']).default('sequential').optional(),
  }));
  // One shared definition preserves strict branch validation without repeating
  // the complete selection contract in each profile and configure operation.
  const branches = (parameters['properties'] as { queries: { items: { anyOf: Array<{ properties: Record<string, unknown> }> } } }).queries.items.anyOf;
  for (const branch of branches) {
    if (branch.properties['capabilities']) branch.properties['capabilities'] = { $ref: '#/definitions/workerCapabilities' };
  }
  parameters['definitions'] = { workerCapabilities: toToolSchema(WorkerCapabilitySelectionSchema.describe('Enabled parent identities; omitted fields use role defaults, [] grants none.')) };

  registerFn(pi, registeredToolNames, {
    name: 'agent',
    label: 'Agent',
    description: DIRECT_TOOL_DESCRIPTIONS.agent!,

    promptSnippet:
      'Spawn or manage bounded workers. Every spawn requires Goal, Context, Scope, Ownership, Acceptance, and Return; the parent must verify and integrate the handback.',
    promptGuidelines: [
      'Delegate when two or more bounded lanes are independent with disjoint ownership, or a specialist materially improves coverage. Keep dependent/shared-file work serial.',
      'Route evidence→researcher, dependency plan→planner, root cause/design→architect, owned code+check→implementer, independent acceptance check→reviewer, CDP evidence→browser; custom requires explicit least-capability tools and systemPrompt.',
      'The tool rejects incomplete packets before creating a worker. Wrong: spawn and reference its unknown agentId in one batch. Right: spawn first; use inspect, wait, message, steer, abort, or kill later.',
      'After spawning, continue non-overlapping parent work; use type:wait to collect results. Verify findings/checks, reconcile an existing plan if present, kill or reuse the worker, and continue the user request. Never trust or persist a raw handback as verified memory.',
      'Standalone delegation needs no plan: omit planStep. Include planStep only when an executing parent plan already owns the work, and copy its exact stable task id; never invent one.',
      'Grant enabled parent identities only. Workers request missing access; configure with snapshotRevision replaces selected arrays. Removals apply now; additions before the next turn.',
      'Discover skill IDs with skill action:list and server/tool pairs with MCPTool action:list. Get the current snapshotRevision with agent type:inspect (no agentId) or capability_revision.',
      'Lean workers expose selected Pi builtins and keep skill/MCP grants empty; use resourceMode:"octocode" for extension tools.',
    ],

    parameters,

    async execute(
      toolCallId: string,
      rawParams: Record<string, unknown>,
      signal?: AbortSignal,
      onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      // Cross-batch reference guard: checked before any item executes.
      const queriesRaw = Array.isArray(rawParams['queries'])
        ? (rawParams['queries'] as QueryRecord[])
        : [];
      rejectCrossBatchReference(queriesRaw);

      return executeQueryBatch({
        toolCallId,
        raw: rawParams,
        signal,
        onUpdate: onUpdate as ((update: ToolCallResult) => void) | undefined,
        ctx,
        passthroughSingle: true,
        preflight: async (query, index) => {
          const type = query['type'] as string | undefined;
          if (!type || !(AGENT_OPERATIONS as readonly string[]).includes(type)) {
            throw new Error(
              `queries[${index}].type must be one of: ${AGENT_OPERATIONS.join(', ')}.`,
            );
          }
          if (type === 'spawn') {
            const profile = String(query['profile'] ?? '').trim();
            if (!(AGENT_PROFILES as readonly string[]).includes(profile)) {
              throw new Error(`queries[${index}].profile must be one of: ${AGENT_PROFILES.join(', ')}.`);
            }
            for (const field of ['goal', 'context', 'scope', 'ownership', 'acceptance', 'returnShape'] as const) {
              if (!String(query[field] ?? '').trim()) throw new Error(`queries[${index}]: spawn requires non-empty ${field}.`);
            }
            if (profile === 'custom') {
              if (!Array.isArray(query['tools'])) throw new Error(`queries[${index}]: custom profile requires tools[].`);
              if (!String(query['systemPrompt'] ?? '').trim()) throw new Error(`queries[${index}]: custom profile requires a non-empty systemPrompt.`);
            }
            return;
          }
          const agentId = String(query['agentId'] ?? '').trim();
          if (type !== 'inspect' && !agentId) {
            throw new Error(`queries[${index}]: ${type} requires agentId.`);
          }
          if ((type === 'message' || type === 'steer') && !String(query['message'] ?? '').trim()) {
            throw new Error(`queries[${index}]: ${type} requires a non-empty message.`);
          }
        },

        async execute(
          query: QueryRecord,
          _index: number,
          _itemId: string,
          itemSignal?: AbortSignal,
          _itemUpdate?: unknown,
          itemCtx?: PiContext,
        ): Promise<ToolCallResult> {
          const type = query['type'] as AgentOperation;
          if (type === 'spawn') return executeSpawnQuery(query, itemCtx, itemSignal);

          return executeAgentLifecycle(query, itemSignal, itemCtx);
        },

        summarize(result: ToolCallResult, query: QueryRecord) {
          const type = query['type'] as string;
          const text =
            (result.content.find((p) => p.type === 'text') as { text?: string } | undefined)
              ?.text ?? '';
          return `${type}: ${text.split('\n').find(Boolean)?.trim().slice(0, 80) ?? 'ok'}`;
        },
      });
    },

    renderCall(rawParams: unknown) {
      const raw = rawParams as {
        queries?: Array<{ type?: string; profile?: string; agentId?: string; task?: string }>;
      };
      const first = raw?.queries?.[0];
      const op = first?.type ?? '…';
      const detail = first?.profile
        ? ` profile:${first.profile}`
        : first?.agentId
          ? ` id:${String(first.agentId).slice(0, 8)}`
          : first?.task
            ? ` "${String(first.task).slice(0, 36)}${String(first.task).length > 36 ? '…' : ''}"`
            : '';
      const extra =
        (raw?.queries?.length ?? 0) > 1
          ? ` +${(raw.queries?.length ?? 1) - 1}`
          : '';
      return makeComponentRenderer((_props, { width: w }) =>
        [truncateToWidth(`agent(${op}${detail})${extra}`, w)], undefined,
      );
    },

    renderResult(result: unknown, _opts: unknown, theme?: PiTheme) {
      const r = result as {
        content?: Array<{ type: string; text?: string }>;
        isError?: boolean;
      };
      const ok = !r?.isError;
      const text =
        (r?.content?.find((p) => p.type === 'text') as { text?: string } | undefined)?.text ?? '';
      const firstLine = text.split('\n').find(Boolean)?.trim() ?? (ok ? 'done' : 'failed');
      const glyph = ok
        ? paint(theme, 'success', CLI_GLYPH.success)
        : paint(theme, 'error', CLI_GLYPH.error);
      return makeComponentRenderer((_props, { width: w }) =>
        [truncateToWidth(`${glyph} agent · ${firstLine}`, w)], undefined,
      );
    },
  } satisfies ToolDefinition);
}
