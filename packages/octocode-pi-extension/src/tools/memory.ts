/**
 * Memory tools — direct calls into @octocodeai/octocode-awareness (no subprocess).
 * Public Pi surface is split by operation; implementation stays shared.
 */
import {
  connectCachedDb,
  getPiAwarenessSessionId,
  MEMORY_LABEL_VALUES,
  resolveDbPath,
  runAwarenessToolOperation,
} from '@octocodeai/octocode-awareness';
import type { AwarenessToolOperation } from '@octocodeai/octocode-awareness';
import type { PiContext, PiTheme, ToolDefinition, ToolCallResult } from '../types.js';
import { buildMemoryRenderCall, buildMemoryRenderResult } from './render-helpers.js';
import { stringEnumSchema } from './schema-helpers.js';
import type { registerUniqueTool } from './octocode-tools.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type Notifier = (ctx: PiContext | undefined, msg: string, level?: string) => void;
type AgentIdResolver = (ctx: PiContext | undefined) => string;
type RegisterFn = typeof registerUniqueTool;

const MEMORY_LABELS = MEMORY_LABEL_VALUES.join('|');
const MEMORY_STATES = ['ACTIVE', 'SUPERSEDED'] as const;
const RECALL_SORTS = ['smart', 'importance', 'recent', 'accessed'] as const;
const REFINEMENT_STATES = ['open', 'ongoing', 'done'] as const;
const NOTIFICATION_KINDS = ['claim', 'handoff', 'question', 'reply', 'blocker', 'request', 'decision', 'fyi'] as const;
const FILE_LOCK_TYPES = ['lock', 'release', 'status', 'renew'] as const;
const FILE_LOCK_KINDS = ['EXCLUSIVE', 'SHARED'] as const;
const AGENT_SIGNAL_ACTIONS = ['publish', 'list', 'reply', 'resolve', 'ack'] as const;


export type MemoryType = AwarenessToolOperation;

function withMemoryDb(
  type: MemoryType,
  params: Record<string, unknown>,
  getAgentId: AgentIdResolver,
  ctx: PiContext | undefined,
): ToolCallResult {
  const db = connectCachedDb(ctx?.dbPath ?? resolveDbPath(null));
  const cwd = ctx?.cwd ?? process.cwd();
  const result = runAwarenessToolOperation(db, type, params, {
    agentId: getAgentId(ctx),
    cwd,
    sessionId: getPiAwarenessSessionId(ctx),
  });
  return {
    content: [{ type: 'text', text: JSON.stringify(result.payload) }],
    details: { exit: result.exitCode },
  };
}

export function executeMemoryOperation(
  type: MemoryType,
  params: Record<string, unknown>,
  getAgentId: AgentIdResolver,
  ctx?: PiContext,
): ToolCallResult {
  try {
    return withMemoryDb(type, params, getAgentId, ctx);
  } catch (err) {
    return {
      content: [{ type: 'text', text: `Failed: ${(err as Error).message}` }],
      details: { exit: 1 },
    };
  }
}

function optionalLimit(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description }));
}

function nonEmptyString(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.String({ minLength: 1, description });
}

function optionalNonEmptyString(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.Optional(nonEmptyString(Type, description));
}

function optionalStringArray(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.Optional(Type.Array(nonEmptyString(Type, description), { description }));
}

function registerMemoryTool(
  getAgentId: AgentIdResolver,
  registerFn: RegisterFn,
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  tool: {
    name: string;
    type: MemoryType;
    label: string;
    description: string;
    promptGuidelines: string[];
    parameters: ToolDefinition['parameters'];
  },
): void {
  registerFn(pi, registeredToolNames, {
    name: tool.name,
    label: tool.label,
    description: tool.description,
    promptSnippet: tool.description,
    promptGuidelines: tool.promptGuidelines,
    parameters: tool.parameters,
    async execute(
      _toolCallId: string,
      params: Record<string, unknown>,
      _signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      return executeMemoryOperation(tool.type, params, getAgentId, ctx);
    },
    renderCall(args: unknown, theme?: PiTheme) {
      return buildMemoryRenderCall(tool.name, args, theme);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      return buildMemoryRenderResult(tool.name, result, opts, theme);
    },
  });
}

export function buildMemoryToolDefinition(
  Type: TypeBoxBuilder,
  getAgentId: AgentIdResolver,
  registerFn: RegisterFn,
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  _notify: Notifier,
): void {
  const labelSchema = Type.Optional(stringEnumSchema(
    Type,
    MEMORY_LABEL_VALUES,
    `Memory category. Allowed: ${MEMORY_LABELS}.`,
  ));
  const importanceSchema = Type.Optional(Type.Integer({
    minimum: 1,
    maximum: 10,
    description: '1–3 minor, 4–6 useful, 7–8 important, 9–10 critical.',
  }));
  const outcomeSchema = stringEnumSchema(Type, ['worked', 'partial', 'failed'], 'worked|partial|failed.');
  const verifyStatusSchema = stringEnumSchema(Type, ['SUCCESS', 'FAILED'], 'SUCCESS or FAILED; default SUCCESS.');
  const memoryStateSchema = stringEnumSchema(Type, MEMORY_STATES, 'ACTIVE (default) or SUPERSEDED.');
  const recallSortSchema = stringEnumSchema(Type, RECALL_SORTS, 'smart (default), importance, recent, or accessed.');
  const refinementStateSchema = stringEnumSchema(Type, REFINEMENT_STATES, 'open|ongoing|done. Default open/ongoing.');
  const notificationKindSchema = stringEnumSchema(Type, NOTIFICATION_KINDS, 'claim|handoff|question|reply|blocker|request|decision|fyi.');
  const fileLockTypeSchema = stringEnumSchema(Type, FILE_LOCK_TYPES, 'lock|release|status|renew.');
  const fileLockKindSchema = stringEnumSchema(Type, FILE_LOCK_KINDS, 'EXCLUSIVE or SHARED; default EXCLUSIVE.');
  const agentSignalActionSchema = stringEnumSchema(Type, AGENT_SIGNAL_ACTIONS, 'publish|list|reply|resolve|ack.');
  const fileScopeProps = {
    file: optionalNonEmptyString(Type, 'Primary related file path.'),
    files: optionalStringArray(Type, 'Related file paths.'),
    folders: optionalStringArray(Type, 'Related folder paths.'),
  };
  // Recall only supports file-scope on the read side (getMemory accepts `files`,
  // not folders) — advertise only what actually filters.
  const recallFileScopeProps = {
    file: optionalNonEmptyString(Type, 'Related file path to scope recall to.'),
    files: optionalStringArray(Type, 'Related file paths to scope recall to.'),
  };
  const workspaceScopeProp = {
    workspace_path: optionalNonEmptyString(Type, 'Workspace/repo root scope; defaults to cwd.'),
  };
  const repoRefProps = {
    repo: optionalNonEmptyString(Type, 'Repository scope, e.g. owner/repo.'),
    ref: optionalNonEmptyString(Type, 'Git ref/branch scope.'),
  };
  // Full write-side scope (workspace + repo + ref) for tools that persist scope.
  const repoScopeProps = { ...workspaceScopeProp, ...repoRefProps };
  const validityProps = {
    valid_from: optionalNonEmptyString(Type, 'Memory valid-from timestamp/ISO date.'),
    valid_to: optionalNonEmptyString(Type, 'Memory expiry timestamp/ISO date; digest marks expired memories stale.'),
  };

  // Shared schema objects — defined once, referenced by canonical + alias tools.
  // Previously file_lock and memory_file_lock each copy-pasted all 12 params.
  const fileLockParams = Type.Object({
    type: fileLockTypeSchema,
    target_files: optionalStringArray(Type, 'Files to lock or release. Relative paths resolve under workspace_path/cwd.'),
    task_id: optionalNonEmptyString(Type, 'Precise task id returned by type:lock. Required for safe release/renew.'),
    lock_type: Type.Optional(fileLockKindSchema),
    ttl_ms: Type.Optional(Type.Integer({ minimum: 1, description: 'Requested lock TTL in milliseconds; capped by awareness.' })),
    reasoning: optionalNonEmptyString(Type, 'Why this lock is needed; shown in lock/status output.'),
    agent_id: optionalNonEmptyString(Type, 'Agent id override; defaults to current Pi agent id.'),
    session_id: optionalNonEmptyString(Type, 'Session id override; defaults to current Pi session id.'),
    status: Type.Optional(Type.String({ description: 'Release status: PENDING, SUCCESS, or FAILED.' })),
    verified: Type.Optional(Type.Boolean({ description: 'For release: mark SUCCESS only if verification actually ran.' })),
    verified_note: optionalNonEmptyString(Type, 'Verification note stored with verified releases.'),
    signal_on_conflict: Type.Optional(Type.Boolean({ description: 'Publish a blocker signal on lock conflict; default true.' })),
    // fileLock scopes by workspace only — repo/ref are not honored upstream.
    ...workspaceScopeProp,
  });

  const workspaceStatusParams = Type.Object({ ...repoScopeProps });

  const tools = [
    {
      name: 'memory_recall',
      type: 'recall' as const,
      label: 'Memory: Recall',
      description: 'Recall durable lessons before risky, unfamiliar, or long-running work.',
      promptGuidelines: [
        'Use before the work only when prior lessons could change the plan.',
        'Skip for routine tasks, obvious one-step edits, or facts already in context.',
      ],
      parameters: Type.Object({
        query: nonEmptyString(Type, 'What you are about to work on, in natural language.'),
        limit: optionalLimit(Type, 'Max memories; default 3.'),
        min_importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Raise to filter low-signal noise.' })),
        smart: Type.Optional(Type.Boolean({ description: 'Broaden after zero results.' })),
        label: labelSchema,
        global_only: Type.Optional(Type.Boolean({ description: 'Search global memories only; skip workspace filtering.' })),
        strict_scope: Type.Optional(Type.Boolean({ description: 'Exact workspace match only; skip NULL-workspace global memories.' })),
        sort: Type.Optional(recallSortSchema),
        state: Type.Optional(memoryStateSchema),
        references: optionalStringArray(Type, 'Filter by exact provenance reference; e.g. npm:pkg, pr:owner/repo#N.'),
        regex: optionalStringArray(Type, 'Regex patterns matched against all text fields.'),
        as_of: optionalNonEmptyString(Type, 'ISO date for bi-temporal point-in-time recall.'),
        // Recall filters by file-scope + workspace only (getMemory ignores repo/ref/folders).
        ...recallFileScopeProps,
        ...workspaceScopeProp,
      }),
    },
    {
      name: 'memory_record',
      type: 'record' as const,
      label: 'Memory: Record',
      description: 'Store a durable root cause, decision, workaround, or verified gotcha.',
      promptGuidelines: [
        'Record only reusable findings that can change future work.',
        'Never store routine status, secrets, raw logs, test output, or facts already in git/docs.',
        'Use supersedes for stale duplicates; allow_similar only for genuinely distinct evidence.',
        'Prefer memory_reflect for post-task lessons — it also creates repo-fix refinements and clusters failure patterns automatically.',
      ],
      parameters: Type.Object({
        task_context: nonEmptyString(Type, 'Why a future agent needs this lesson.'),
        observation: nonEmptyString(Type, 'Durable lesson: X caused Y because Z — do A; verify with B.'),
        label: labelSchema,
        importance: importanceSchema,
        tags: optionalStringArray(Type, 'Recall keywords.'),
        references: optionalStringArray(Type, 'Provenance such as file:/abs/path:line, pr:owner/repo#N, URL, npm:pkg@v.'),
        ...fileScopeProps,
        ...repoScopeProps,
        ...validityProps,
        supersedes: Type.Optional(Type.Union([
          nonEmptyString(Type, 'Stale memory id this one replaces.'),
          Type.Array(nonEmptyString(Type, 'Stale memory id this one replaces.'), { description: 'Stale memory id(s) this one replaces.' }),
        ], { description: 'Stale memory id(s) this one replaces.' })),
        allow_similar: Type.Optional(Type.Boolean({ description: 'Bypass duplicate skip only for distinct new evidence.' })),
        failure_signature: optionalNonEmptyString(Type, 'Cluster key, e.g. mechanism:X|cause:Y.'),
      }),
    },
    {
      name: 'memory_reflect',
      type: 'reflect' as const,
      label: 'Memory: Reflect',
      description: 'Capture a reusable lesson after completing work. Prefer over memory_record when fix_repo, fix_harness, or failure_signature apply — those create refinements and cluster failure patterns automatically.',
      promptGuidelines: [
        'Prefer over memory_record when fix_repo, fix_harness, or failure_signature are relevant.',
        'Skip if there is no lesson, no failure pattern, and no repo/harness fix to propagate.',
      ],
      parameters: Type.Object({
        task: nonEmptyString(Type, 'Task just completed.'),
        outcome: Type.Optional(outcomeSchema),
        lesson: optionalNonEmptyString(Type, 'Durable reusable lesson; omit if none.'),
        worked: optionalNonEmptyString(Type, 'Concise note on what worked.'),
        didnt_work: optionalNonEmptyString(Type, 'Concise failure; used as lesson if lesson is omitted.'),
        fix_repo: optionalNonEmptyString(Type, 'Concrete repo-fix note; creates an open refinement.'),
        fix_harness: optionalNonEmptyString(Type, 'Harness/skill improvement; creates a harness-tagged memory.'),
        failure_signature: optionalNonEmptyString(Type, 'Cluster key, e.g. mechanism:X|cause:Y.'),
        importance: importanceSchema,
        judgment_note: optionalNonEmptyString(Type, 'Evidence checked + remaining uncertainty; folded into the reflection narrative.'),
        duo: Type.Optional(Type.Boolean({ description: 'Emit an advisory reflection_duo packet (supporter + skeptic prompts). Never stored.' })),
        eval_failures: Type.Optional(Type.Array(Type.Object({
          id: nonEmptyString(Type, 'Eval question/check id.'),
          dimension: optionalNonEmptyString(Type, 'Eval dimension, e.g. correctness.'),
          failure_signature: optionalNonEmptyString(Type, 'Cluster key for mine-weakness.'),
          suggested_lesson: optionalNonEmptyString(Type, 'Distilled lesson from the failed check.'),
        }), { description: 'Structured failed eval checks; each becomes an eval-tagged memory.' })),
        references: optionalStringArray(Type, 'Provenance such as file:/abs/path:line, pr:owner/repo#N, URL, npm:pkg@v.'),
        ...fileScopeProps,
        ...repoScopeProps,
        ...validityProps,
      }),
    },
    {
      name: 'workspace_status',
      type: 'workspace_status' as const,
      label: 'Workspace Status',
      description: 'Show active file locks, working agents, open signals/refinements, and memory store stats for the current workspace.',
      promptGuidelines: [
        'Use to check if another agent is editing files you need, or to see what is locked.',
        'Use before long edits to verify no conflicts exist.',
      ],
      parameters: workspaceStatusParams,
    },
    {
      name: 'agent_signal',
      type: 'agent_signal' as const,
      label: 'Agent Signal',
      description: 'Common agent coordination inbox: publish/list/reply/resolve questions, handoffs, blockers, decisions, and FYIs.',
      promptGuidelines: [
        'Use for agent-to-agent coordination: questions, replies, handoffs, blockers, decisions, FYIs.',
        'Use list to inspect unread signals; use reply/resolve to close loops instead of creating ad-hoc tools.',
        'This is an awareness inbox, not the source of truth for locks or verification.',
        'Use action:"ack" after processing a signal so hook delivery can safely replay until acknowledged.',
      ],
      parameters: Type.Object({
        action: agentSignalActionSchema,
        kind: Type.Optional(notificationKindSchema),
        subject: optionalNonEmptyString(Type, 'One-line signal subject for publish/reply.'),
        body: optionalNonEmptyString(Type, 'Optional detail.'),
        to_agents: optionalStringArray(Type, 'Recipient agent ids; omit/empty for broadcast.'),
        files: optionalStringArray(Type, 'Files this signal concerns.'),
        refs: optionalStringArray(Type, 'Related ids or references: memory ids, task ids, signal ids, URLs, PRs.'),
        importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Importance 1-10; default 5.' })),
        in_reply_to: optionalNonEmptyString(Type, 'Parent signal id for reply threading.'),
        thread_id: optionalNonEmptyString(Type, 'Thread id for list/resolve.'),
        signal_ids: optionalStringArray(Type, 'Signal ids for resolve/ack.'),
        unread_only: Type.Optional(Type.Boolean({ description: 'List only unread/open signals; default true.' })),
        mark_read: Type.Optional(Type.Boolean({ description: 'Mark listed signals as read.' })),
        kinds: Type.Optional(Type.Array(notificationKindSchema, { description: 'Filter list by signal kind.' })),
        limit: optionalLimit(Type, 'Max signals; default 20.'),
        agent_id: optionalNonEmptyString(Type, 'Agent id override; defaults to current Pi agent id.'),
        ...repoScopeProps,
      }),
    },
    {
      name: 'file_lock',
      type: 'file_lock' as const,
      label: 'File Lock',
      description: 'Manage file locks for parallel agents. type lock/release/status/renew; uses task_id as the safe release handle.',
      promptGuidelines: [
        'Prefer automatic edit/write locks; use this for explicit coordination across parallel agents.',
        'Release and renew by task_id whenever possible; agentId/sessionId are scope metadata, not precise lock handles.',
        'Set ttl_ms for bounded work; locks are capped by awareness to the maximum safe TTL.',
        'Include reasoning so status output explains why the files are locked.',
      ],
      parameters: fileLockParams,
    },
    {
      name: 'memory_workspace_status',
      type: 'workspace_status' as const,
      label: 'Memory: Workspace Status',
      description: 'Compatibility alias for workspace_status.',
      promptGuidelines: [
        'Prefer workspace_status for new usage; this alias is retained for compatibility.',
      ],
      parameters: workspaceStatusParams,
    },
    {
      name: 'memory_file_lock',
      type: 'file_lock' as const,
      label: 'Memory: File Lock',
      description: 'Compatibility alias for file_lock.',
      promptGuidelines: [
        'Prefer file_lock for new usage; this alias is retained for compatibility.',
      ],
      parameters: fileLockParams,
    },
    {
      name: 'memory_refine_get',
      type: 'refine_get' as const,
      label: 'Memory: Refinements',
      description: 'List open repo-fix refinements for the current workspace.',
      promptGuidelines: [
        'Use before related work when previous reflections may have left actionable fixes.',
        'Use memory_recall for broad prior lessons instead.',
      ],
      parameters: Type.Object({
        state: Type.Optional(refinementStateSchema),
        include_handoffs: Type.Optional(Type.Boolean({ description: 'Include session handoff rows; default false so repo-fix refinements stay visible.' })),
        limit: optionalLimit(Type, 'Max refinements; default 5.'),
        // getRefinements scopes by workspace + repo (no ref).
        ...workspaceScopeProp,
        repo: repoRefProps.repo,
      }),
    },
    {
      name: 'memory_audit_unverified',
      type: 'audit_unverified' as const,
      label: 'Memory: Audit Unverified',
      description: 'List pending edit tasks that still need verification. Auto-fires on agent_end; call manually only for a mid-turn check.',
      promptGuidelines: [
        'Run mid-turn when you suspect unverified edits; the agent_end hook already performs the final audit.',
        'If pending tasks exist, run the stated checks and clear with memory_verify({task_ids:[...], status}) for batch, memory_verify({allPending:true}) to clear all, or memory_verify({task_id, status}) for one.',
      ],
      parameters: Type.Object({}),
    },
    {
      name: 'memory_verify',
      type: 'verify' as const,
      label: 'Memory: Verify Task',
      description: 'Mark a pending edit task as verified or failed after running its check. Accepts a single task_id, a batch task_ids[] array, or allPending:true to clear every pending task for this agent in one call.',
      promptGuidelines: [
        'Use only after running the stated verification for the task.',
        'Never mark SUCCESS just to clear the gate.',
        'Prefer task_ids[] or allPending:true to clear multiple tasks in a single tool call instead of looping.',
      ],
      parameters: Type.Object({
        task_id: optionalNonEmptyString(Type, 'Single pending task id to verify.'),
        task_ids: Type.Optional(Type.Array(nonEmptyString(Type, 'Pending task id to verify.'), { minItems: 1, description: 'Batch: list of pending task ids to verify in one call.' })),
        allPending: Type.Optional(Type.Boolean({ description: 'Verify ALL pending tasks for this agent in one call. Pair with status.' })),
        status: Type.Optional(verifyStatusSchema),
      }),
    },
    {
      name: 'memory_export_harness',
      type: 'export_harness' as const,
      label: 'Memory: Export Harness',
      description: 'Export agent improvement proposals for AGENTS.md or CLAUDE.md. Tier 1: explicit harness proposals from memory_reflect fix_harness: (always first). Tier 2: high-importance general lessons. Raw reflections excluded. Never writes files — review and paste after human approval.',
      promptGuidelines: [
        'Never paste output into AGENTS.md without human review and explicit approval.',
        'Use harness_only:true to see only explicit fix_harness proposals, not general lessons.',
        'Route recurring failures through memory_reflect (fix_harness) first so this export has proposals to surface.',
      ],
      parameters: Type.Object({
        harness_only: Type.Optional(Type.Boolean({ description: 'Return only harness-tagged proposals (tier 1). Omit general lessons.' })),
        limit: optionalLimit(Type, 'Max memories; default 10.'),
        min_importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Minimum importance for tier 2 general lessons; default 7.' })),
        // exportHarness scopes by workspace only (no repo/ref upstream).
        ...workspaceScopeProp,
      }),
    },
    {
      name: 'memory_notify',
      type: 'notify' as const,
      label: 'Memory: Notify',
      description: 'Compatibility alias for agent_signal({action:"publish"}). Prefer agent_signal for list/reply/resolve.',
      promptGuidelines: [
        'Prefer agent_signal for new coordination; memory_notify only publishes a signal.',
        'Use for simple legacy handoffs/blockers/questions when no reply/list/resolve is needed.',
      ],
      parameters: Type.Object({
        kind: notificationKindSchema,
        subject: nonEmptyString(Type, 'One-line summary of the message.'),
        body: optionalNonEmptyString(Type, 'Optional detail.'),
        to_agent: optionalNonEmptyString(Type, 'Recipient agent id; omit to broadcast to all agents on this workspace.'),
        files: optionalStringArray(Type, 'Files this message concerns.'),
        importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Importance 1-10; default 5.' })),
        ...repoScopeProps,
      }),
    },
  ];

  for (const tool of tools) {
    registerMemoryTool(getAgentId, registerFn, pi, registeredToolNames, tool);
  }
}
