/**
 * callTool — a self-extending meta-tool.
 *
 * The agent asks for a *capability* (`toolType` + `metadata`). callTool:
 *   1. resolves whether a matching dynamic tool already exists (O(1), in-process)
 *   2. reuses it (isolated subprocess run) when found
 *   3. otherwise generates a verified tool via a tool-smith subagent, registers it
 *      only if its generated test passes, then runs it
 *   4. enhances/fixes an existing tool on demand (versioned, verification-gated)
 *
 * Deterministic mechanics (registry, resolve, verification gate, checksum, sandbox
 * execution) live in dynamic-tools.ts. This file owns the LLM-driven codegen and the
 * orchestration. The codegen step is injectable (setToolGeneratorForTests) so the
 * orchestration is testable without spawning real workers.
 */

import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import { sliceBetween } from '../utils.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { spawnRpcAgent, waitForAgentTurn, isSubagentProcess, killWorkerById } from './agent-tools.js';
import { requestApproval } from './approval.js';
import {
  resolveTool,
  registerGeneratedTool,
  runDynamicTool,
  recordUsage,
  deleteTool,
  listTools,
  getRegistryDir,
  type Capability,
  type ToolManifestEntry,
} from './dynamic-tools.js';
import fs from 'node:fs';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

type Mode = 'auto' | 'run' | 'create' | 'enhance' | 'fix' | 'list' | 'delete';

interface CallToolParams {
  toolType: string;
  metadata?: Record<string, unknown>;
  mode?: Mode;
}

// ─── codegen contract (injectable for tests) ──────────────────────────────────

export interface GeneratedTool {
  name: string;
  description: string;
  keywords: string[];
  capabilities: Capability[];
  /** Why this tool should exist as a reusable capability. */
  reason: string;
  /** Enforce OS-level isolation (default true). Only trusted broad-access tools set false. */
  sandboxed?: boolean;
  /** Pure function of metadata (same input → same output, no side effects). Enables result memoization. */
  deterministic?: boolean;
  source: string;
  test: string;
}

// ─── triviality guard ─────────────────────────────────────────────────────────

/**
 * A dynamic tool must EARN its existence. If a one-line shell command already does
 * the job, creating a persisted tool is pure overhead — it should optimize the
 * agent, not bloat it. This heuristic maps common trivial capabilities to the shell
 * command that already covers them. Matched requests are declined (unless forced).
 *
 * Heuristic, not exhaustive: keyed on obvious keywords in toolType+intent. The
 * upgrade path is `metadata._force:true` when the caller genuinely needs a persisted,
 * parameterized, verified version.
 */
const TRIVIAL_PATTERNS: Array<{ words: string[]; suggestion: string }> = [
  { words: ['time', 'date', 'now', 'clock', 'timestamp'], suggestion: 'date (e.g. `date -u +%FT%TZ`)' },
  { words: ['uuid', 'guid'], suggestion: 'uuidgen' },
  { words: ['base64', 'b64'], suggestion: 'base64 / base64 -d' },
  { words: ['hostname'], suggestion: 'hostname' },
  { words: ['whoami', 'username'], suggestion: 'whoami' },
  { words: ['cwd', 'pwd', 'workingdir'], suggestion: 'pwd' },
  { words: ['wordcount', 'linecount', 'countlines', 'countwords'], suggestion: 'wc' },
  { words: ['md5', 'sha1', 'sha256', 'checksum', 'hash'], suggestion: 'shasum / md5sum' },
  { words: ['random', 'randomnumber'], suggestion: '$RANDOM or `od -An -N4 -tu4 /dev/urandom`' },
  { words: ['jsonpretty', 'prettyjson', 'formatjson'], suggestion: 'jq .' },
  { words: ['echo', 'print', 'constant'], suggestion: 'echo' },
];

/**
 * Tokenize a capability name + intent into whole words. Splits camelCase
 * (`updateData` → `update`, `data`) and separators, but keeps letter/digit runs
 * together (`base64` stays one token). Whole-word matching avoids substring
 * false positives like `updateData` hitting `date` or `getNow` hitting `now`
 * via a naive `includes`.
 */
function tokenizeTriviality(text: string): Set<string> {
  const spaced = text.replace(/([a-z0-9])([A-Z])/g, '$1 $2');
  return new Set(
    spaced
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter(Boolean),
  );
}

export function assessTriviality(
  toolType: string,
  intent: string,
): { trivial: boolean; suggestion?: string } {
  const tokens = tokenizeTriviality(`${toolType} ${intent}`);
  for (const p of TRIVIAL_PATTERNS) {
    if (p.words.some((w) => tokens.has(w))) return { trivial: true, suggestion: p.suggestion };
  }
  return { trivial: false };
}

// ─── auto-maintenance ─────────────────────────────────────────────────────────

/**
 * Remove unambiguous junk from the registry. Runs opportunistically on every call so
 * the tool library stays lean without manual upkeep. Only clearly-dead tools are
 * pruned: a missing/broken entry file, or a tool that has failed every time it ran
 * (>=3 attempts). Anything ambiguous is left for explicit `mode:"delete"`.
 */
export function sweepJunk(dir = getRegistryDir()): string[] {
  const pruned: string[] = [];
  for (const entry of listTools(dir)) {
    const missing = !fs.existsSync(entry.entry);
    const alwaysFails = entry.stats.calls >= 3 && entry.stats.failures === entry.stats.calls;
    if (missing || alwaysFails) {
      if (deleteTool(entry.name, dir)) pruned.push(entry.name);
    }
  }
  return pruned;
}

export interface GenerateArgs {
  toolType: string;
  intent: string;
  metadata: Record<string, unknown>;
  mode: Mode;
  existing?: ToolManifestEntry;
  model?: string;
  ctx?: PiContext;
}

export type ToolGenerator = (args: GenerateArgs) => Promise<GeneratedTool>;

let _generator: ToolGenerator | null = null;

/** Test hook: inject a deterministic generator instead of spawning a tool-smith worker. */
export function setToolGeneratorForTests(gen: ToolGenerator | null): void {
  _generator = gen;
}

// ─── tool-smith worker (default generator) ─────────────────────────────────────

const SENTINELS = {
  manifest: '===MANIFEST===',
  source: '===SOURCE===',
  test: '===TEST===',
  end: '===END===',
} as const;

function buildToolSmithPrompt(a: GenerateArgs): string {
  const lines = [
    a.mode === 'enhance' || a.mode === 'fix'
      ? `Improve the existing dynamic tool "${a.toolType}".`
      : `Create a new dynamic tool named "${a.toolType}".`,
    '',
    `Intent: ${a.intent || '(infer from the tool name and metadata)'}`,
    `Sample metadata (the runtime input shape): ${JSON.stringify(a.metadata)}`,
  ];
  if (a.existing) {
    lines.push('', `Current version: ${a.existing.version}. Fix/enhance it, keep the same input contract where possible.`);
  }
  lines.push(
    '',
    'Before writing, briefly research and brainstorm: is there a Node built-in or a well-known, robust approach for this? Prefer the simplest correct design. Do NOT reinvent what the platform already provides.',
    '',
    'Requirements:',
    '- Emit a self-contained ES module with a default export: `export default async function (metadata) { ... }` that returns a JSON-serializable value.',
    '- Use only Node.js built-in modules. Do NOT add npm dependencies.',
    '- Declare capabilities honestly: "net" (network), "fs" (broad filesystem), "exec" (child processes). Use [] when none are needed — the tool runs OS-sandboxed and only declared+approved capabilities are granted.',
    '- Keep `sandboxed` true (default). Only set it false for a trusted tool that genuinely needs broad host access; this must be justified in `reason`.',
    '- Set `deterministic:true` ONLY for a pure function of its input (same metadata → same result, no clock/network/random/fs). This memoizes results across identical calls.',
    '- Provide a test module that imports the tool and exits 0 on success, non-zero on failure. Import path placeholder: use `./tool.mjs`.',
    '- Prefer deterministic, dependency-free implementations.',
    '- Include a short `reason`: why this deserves a persisted, reusable tool.',
    '',
    'Output EXACTLY these four fenced sections and nothing else outside them:',
    SENTINELS.manifest,
    '{"name":"<name>","description":"<one line>","keywords":["..."],"capabilities":[],"sandboxed":true,"deterministic":false,"reason":"<why this tool should exist>"}',
    SENTINELS.source,
    '<tool.mjs source>',
    SENTINELS.test,
    '<tool.test.mjs source, importing from "./tool.mjs">',
    SENTINELS.end,
  );
  return lines.join('\n');
}

/** Parse a tool-smith worker's output into a GeneratedTool. Throws on malformed output. */
export function parseGeneratedTool(output: string, fallbackName: string): GeneratedTool {
  const manifestRaw = sliceBetween(output, SENTINELS.manifest, SENTINELS.source);
  const source = stripFences(sliceBetween(output, SENTINELS.source, SENTINELS.test));
  const test = stripFences(sliceBetween(output, SENTINELS.test, SENTINELS.end));
  if (!source || !test) throw new Error('tool-smith output missing SOURCE or TEST section');
  let manifest: Partial<GeneratedTool> = {};
  try {
    manifest = JSON.parse(manifestRaw || '{}');
  } catch {
    throw new Error('tool-smith MANIFEST section is not valid JSON');
  }
  const caps = (Array.isArray(manifest.capabilities) ? manifest.capabilities : []).filter(
    (c): c is Capability => c === 'net' || c === 'fs' || c === 'exec',
  );
  // The test placeholder imports "./tool.mjs"; registerGeneratedTool writes both files
  // into the same directory, so a relative import resolves correctly.
  return {
    name: (manifest.name as string) || fallbackName,
    description: (manifest.description as string) || fallbackName,
    keywords: Array.isArray(manifest.keywords) ? (manifest.keywords as string[]) : [],
    capabilities: caps,
    reason: (manifest.reason as string) || '',
    sandboxed: manifest.sandboxed !== false,
    deterministic: manifest.deterministic === true,
    source,
    test,
  };
}

function stripFences(s: string): string {
  return s
    .replace(/^```[a-zA-Z]*\n/, '')
    .replace(/\n```$/, '')
    .trim();
}

const defaultGenerator: ToolGenerator = async (a) => {
  const record = spawnRpcAgent(
    {
      task: buildToolSmithPrompt(a),
      name: `tool-smith · ${a.toolType}`,
      tools: [],
      resourceMode: 'lean',
      systemPrompt:
        'You are a tool-smith. You write small, dependency-free, deterministic Node.js ES modules ' +
        'and their tests. Emit only the four sentinel-delimited sections requested. No prose.',
      model: a.model,
      noSession: true,
    },
    a.ctx,
  );
  try {
    // Progress-aware: reset on every event and probe on quiet gaps so a long-but-
    // active tool-smith turn runs to completion, with an absolute backstop against
    // a genuinely hung worker wedging the main process.
    await waitForAgentTurn(record, { maxSilenceMs: 120_000, absoluteCapMs: 600_000 });
    const output = record.lastOutput || record.stderr || '';
    return parseGeneratedTool(output, a.toolType);
  } finally {
    // On timeout/error the spawned smith worker is still alive — kill it so it
    // does not orphan, and its record becomes droppable (reclaimable slot).
    // On success the process has already exited, so this is a harmless no-op.
    killWorkerById(record.id);
  }
};

// ─── orchestration ─────────────────────────────────────────────────────────────

function getGenerator(): ToolGenerator {
  return _generator ?? defaultGenerator;
}

function approvedCapabilities(metadata: Record<string, unknown>): Capability[] {
  const raw = metadata['_allow'];
  if (!Array.isArray(raw)) return [];
  return raw.filter((c): c is Capability => c === 'net' || c === 'fs' || c === 'exec');
}

function stripReservedKeys(metadata: Record<string, unknown>): Record<string, unknown> {
  const { _allow, _approveCreate, _force, _sandboxed, intent, reason, ...rest } = metadata;
  void _allow;
  void _approveCreate;
  void _force;
  void _sandboxed;
  void intent;
  void reason;
  return rest;
}

async function approveSandboxOptOut(ctx: PiContext | undefined, toolType: string, intent: string) {
  return await requestApproval(ctx, {
    actionClass: 'system',
    title: 'Create non-sandboxed dynamic tool',
    detail: [`toolType: ${toolType}`, intent ? `intent: ${intent}` : undefined].filter(Boolean).join('\n'),
  });
}

interface OrchestrateOutcome {
  status:
    | 'ran'
    | 'created-and-ran'
    | 'blocked'
    | 'error'
    | 'proposal'
    | 'declined'
    | 'listed'
    | 'deleted';
  toolName?: string;
  hit?: 'exact' | 'keyword' | 'miss';
  result?: unknown;
  message?: string;
  /** Junk tool names pruned by the maintenance sweep on this call. */
  pruned?: string[];
  /** True when the result came from the deterministic memoization cache. */
  cached?: boolean;
  /** Tool inventory for mode:"list". */
  tools?: Array<{ name: string; description: string; version: number; calls: number; failures: number }>;
}

async function orchestrate(params: CallToolParams, ctx?: PiContext): Promise<OrchestrateOutcome> {
  const metadata = params.metadata ?? {};
  const mode: Mode = params.mode ?? 'auto';
  const intent = typeof metadata['intent'] === 'string' ? (metadata['intent'] as string) : '';
  const reason = typeof metadata['reason'] === 'string' ? (metadata['reason'] as string) : '';
  const approveCreate = metadata['_approveCreate'] === true;
  const force = metadata['_force'] === true;
  const allow = approvedCapabilities(metadata);
  const args = stripReservedKeys(metadata);

  // Always maintain: prune unambiguous junk on every call so the library stays lean.
  const pruned = sweepJunk();

  // ── CRUD read: list inventory ──
  if (mode === 'list') {
    return {
      status: 'listed',
      pruned,
      tools: listTools().map((t) => ({
        name: t.name,
        description: t.description,
        version: t.version,
        calls: t.stats.calls,
        failures: t.stats.failures,
      })),
    };
  }

  // ── CRUD delete ──
  if (mode === 'delete') {
    const ok = deleteTool(params.toolType);
    return {
      status: ok ? 'deleted' : 'error',
      toolName: params.toolType,
      pruned,
      message: ok ? `Deleted tool "${params.toolType}".` : `No tool named "${params.toolType}" to delete.`,
    };
  }

  const resolved = resolveTool(params.toolType, intent);

  if (mode === 'run' && resolved.hit === 'miss') {
    return { status: 'error', hit: 'miss', pruned, message: `No tool matches "${params.toolType}" and mode:"run" won't create one.` };
  }

  let entry: ToolManifestEntry | undefined =
    resolved.hit === 'exact' || resolved.hit === 'keyword' ? resolved.entry : undefined;

  const explicitCreate = mode === 'create' || mode === 'enhance' || mode === 'fix';
  const wantsCreate = explicitCreate || (mode === 'auto' && resolved.hit === 'miss');

  if (wantsCreate) {
    if ((mode === 'enhance' || mode === 'fix') && !entry) {
      return { status: 'error', pruned, message: `mode:"${mode}" requires an existing tool named "${params.toolType}".` };
    }

    // Triviality guard: a tool must optimize the agent, not bloat it. If a one-line
    // shell command already covers this, decline (unless _force).
    const triv = assessTriviality(params.toolType, intent);
    if (triv.trivial && !force) {
      return {
        status: 'declined',
        toolName: params.toolType,
        pruned,
        message:
          `Not creating "${params.toolType}": a simple command already does this — use \`${triv.suggestion}\`. ` +
          `If you truly need a persisted, parameterized, verified tool, re-call with metadata._force:true.`,
      };
    }

    // Approval-gated creation (nice UX): on an auto-mode miss we do NOT silently
    // generate. We propose creation and ask the user to approve, after research/
    // brainstorm. Approval = mode:"create" OR metadata._approveCreate:true.
    const approved = explicitCreate || approveCreate;
    if (!approved) {
      return {
        status: 'proposal',
        toolName: params.toolType,
        hit: resolved.hit,
        pruned,
        message:
          `No tool for "${params.toolType}". Before creating one: research whether a built-in/library or a ` +
          `simple command already covers it, and brainstorm the smallest design. If a new reusable tool is ` +
          `justified, ask the user to confirm, then re-call with mode:"create" (and metadata.reason explaining why).`,
      };
    }

    if (!reason.trim()) {
      return {
        status: 'error',
        toolName: params.toolType,
        pruned,
        message: 'Tool creation requires metadata.reason explaining why this reusable tool should exist.',
      };
    }

    let generated: GeneratedTool;
    try {
      generated = await getGenerator()({ toolType: params.toolType, intent, metadata, mode, existing: entry, ctx });
    } catch (err) {
      return { status: 'error', pruned, message: `Tool generation failed: ${(err as Error).message}` };
    }
    // A tool may only opt OUT of the sandbox after the shared approval gate says yes.
    // Non-interactive hosts fail closed through requestApproval().
    const sandboxed = metadata['_sandboxed'] !== false;
    if (!sandboxed) {
      const approval = await approveSandboxOptOut(ctx, params.toolType, intent);
      if (!approval.approved) {
        const why = approval.interactive
          ? 'The user declined this action.'
          : 'This host is non-interactive, so approval could not be collected.';
        return {
          status: 'blocked',
          toolName: params.toolType,
          pruned,
          message: `Non-sandboxed dynamic tool creation requires explicit user approval. ${why}`,
        };
      }
    }
    const reg = registerGeneratedTool({ ...generated, reason: generated.reason || reason, sandboxed, deterministic: generated.deterministic });
    if (!reg.ok) {
      return {
        status: 'error',
        toolName: generated.name,
        pruned,
        message: `Generated tool rejected by verification gate (${reg.reason}${reg.detail ? `: ${reg.detail}` : ''}).`,
      };
    }
    entry = reg.entry;
  }

  if (!entry) return { status: 'error', hit: resolved.hit, pruned, message: 'No tool available to run.' };

  // Capability approval gate: block (don't silently run) when the tool needs a
  // capability the caller hasn't approved via metadata._allow.
  const missingCaps = entry.capabilities.filter((c) => !allow.includes(c));
  if (missingCaps.length > 0) {
    return {
      status: 'blocked',
      toolName: entry.name,
      pruned,
      message:
        `Tool "${entry.name}" needs capabilities [${missingCaps.join(', ')}]. ` +
        `Re-call with metadata._allow: [${missingCaps.map((c) => `"${c}"`).join(', ')}] to approve.`,
    };
  }

  const run = runDynamicTool(entry, args, { allow });
  recordUsage(entry.name, run.ok);
  if (!run.ok) {
    return { status: 'error', toolName: entry.name, pruned, message: `Execution failed: ${run.reason}${run.detail ? ` — ${run.detail}` : ''}` };
  }
  return {
    status: wantsCreate ? 'created-and-ran' : 'ran',
    cached: run.cached === true,
    toolName: entry.name,
    hit: resolved.hit,
    pruned,
    result: run.result,
  };
}

function renderHeader(outcome: OrchestrateOutcome): string {
  switch (outcome.status) {
    case 'ran':
      return `[REUSED] ${outcome.toolName} (${outcome.hit}${outcome.cached ? ', cached' : ''})`;
    case 'created-and-ran':
      return `[CREATED] ${outcome.toolName}`;
    case 'proposal':
      return `[PROPOSAL] ${outcome.message}`;
    case 'declined':
      return `[DECLINED] ${outcome.message}`;
    case 'blocked':
      return `[BLOCKED] ${outcome.message}`;
    case 'listed':
      return `[TOOLS] ${(outcome.tools ?? []).length} dynamic tool(s)`;
    case 'deleted':
      return `[DELETED] ${outcome.message}`;
    default:
      return `[ERROR] ${outcome.message}`;
  }
}

// ─── registration ─────────────────────────────────────────────────────────────

export function registerCallTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  // Workers cannot spawn workers; the tool-smith path spawns a subagent, so never
  // register callTool inside a spawned worker process.
  if (isSubagentProcess()) return;

  registerFn(pi, registeredToolNames, {
    name: 'callTool',
    label: 'Call Tool',
    description: [
      'Meta-tool: request a capability by name and callTool reuses, creates, or maintains a verified dynamic tool to satisfy it.',
      'Resolves an existing tool in O(1); on a miss it PROPOSES creation (it does not silently generate). After you research/brainstorm and the user confirms, re-call with mode:"create" to generate a self-contained tool via a tool-smith subagent, which is registered ONLY if its generated test passes, then run in an isolated subprocess.',
      '',
      'Modes: auto (default: reuse, else propose) · run (reuse only) · create (generate after approval) · enhance/fix (regenerate an existing tool) · list (inventory) · delete (remove a tool).',
      'Every call also prunes unambiguous junk (missing/always-failing tools) to keep the library lean.',
      '',
      'Use ONLY for small, reusable, deterministic capabilities. A tool must optimize the agent, not bloat it: if a one-line shell command already does the job, callTool declines and points you to it — do not create a tool for trivial one-offs.',
      '',
      'metadata carries runtime args AND reserved keys: `intent` (what a new tool should do), `reason` (REQUIRED to create), `_allow` (approve net/fs/exec), `_force` (override the triviality decline), `_approveCreate` (approve creation in auto mode), `_sandboxed:false` (request explicit approval for creating a NON-sandboxed trusted tool — rare).',
      'Generated code runs OS-sandboxed by default (Node permission model: denied-by-default fs/net/child_process, scrubbed env), plus hard timeout and checksum tamper-check. Declared capabilities are ENFORCED, not just advisory.',
    ].join('\n'),
    promptSnippet: 'Reuse, propose, or maintain a verified dynamic tool for a requested capability',
    promptGuidelines: [
      'Use callTool for reusable deterministic capabilities; never for trivial one-offs a shell command or direct reasoning already covers — tools must optimize the agent, not bloat it.',
      'On a creation proposal: first research (built-in? library? existing tool? simple command?) and brainstorm the smallest design, then ASK the user to confirm before re-calling with mode:"create" and a clear metadata.reason.',
      'Maintain the library: it auto-prunes junk each call; use mode:"list" to review and mode:"delete" to remove obsolete or superseded tools.',
      'Generated tools are verification-gated (their test must pass) and sandboxed; approve net/fs/exec explicitly via metadata._allow only when required.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, Type.Object({
      toolType: Type.String({
        description: 'Logical name of the capability, e.g. "getCurrentTime", "toSlug", "uuidV4". Used as the O(1) registry key.',
      }),
      metadata: Type.Optional(
        Type.Unsafe({
          type: 'object',
          additionalProperties: true,
          description:
            'Runtime input args for the tool. Reserved keys: `intent` (natural-language description used to generate a missing tool), `_allow` (array approving capabilities like ["net"]), and `_sandboxed:false` (request explicit approval for a rare non-sandboxed trusted tool).',
        }),
      ),
      mode: Type.Optional(
        Type.Unsafe({
          type: 'string',
          enum: ['auto', 'run', 'create', 'enhance', 'fix', 'list', 'delete'],
          description:
            'auto (default): reuse or create. run: reuse only, error on miss. create: force (re)generate. enhance/fix: regenerate an existing tool (version bump). list: inventory. delete: remove a tool.',
        }),
      ),
    }, { additionalProperties: false }), {
      reasoningDescription: 'Concise reason this dynamic tool operation is necessary.',
    }),

    async execute(id: string, rawParams: Record<string, unknown>, signal, onUpdate, ctx?: PiContext) {
      const queryCount = Array.isArray(rawParams.queries) ? rawParams.queries.length : 0;
      return executeQueryBatch({
        toolCallId: id,
        raw: rawParams,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        preflight: queryCount > 1
          ? (query) => {
              const params = query as unknown as CallToolParams;
              if (!params.toolType?.trim()) throw new Error('toolType must be a non-empty string.');
              const mode = params.mode ?? 'auto';
              if ((mode === 'create' || mode === 'enhance' || mode === 'fix') && !String(params.metadata?.['reason'] ?? '').trim()) {
                throw new Error(`mode:"${mode}" requires metadata.reason.`);
              }
            }
          : undefined,
        async execute(query) {
          const outcome = await orchestrate(query as unknown as CallToolParams, ctx);
          const header = renderHeader(outcome);
          const parts: string[] = [header];
          if (outcome.status === 'ran' || outcome.status === 'created-and-ran') {
            parts.push(JSON.stringify(outcome.result, null, 2));
          }
          if (outcome.status === 'listed') {
            parts.push(
              (outcome.tools ?? [])
                .map((t) => `  ${t.name} v${t.version} — ${t.description} (calls ${t.calls}, fails ${t.failures})`)
                .join('\n') || '  (no dynamic tools)',
            );
          }
          if (outcome.pruned && outcome.pruned.length > 0) {
            parts.push(`[MAINTAINED] pruned junk: ${outcome.pruned.join(', ')}`);
          }
          return {
            content: [{ type: 'text', text: parts.join('\n') }],
            isError: outcome.status === 'error',
            details: outcome,
          } as unknown as ToolCallResult;
        },
      });
    },

    renderCall(rawParams: unknown, theme?: PiTheme) {
      const envelope = rawParams && typeof rawParams === 'object' ? rawParams as Record<string, unknown> : {};
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as CallToolParams[] : [];
      const p = queries[0] ?? {} as CallToolParams;
      // Brand title + dim args, matching the other tool-call rows.
      return buildToolView({
        name: 'callTool',
        state: 'request',
        segments: [
          { text: String(p.toolType ?? 'capability'), token: 'symbol' },
          ...(p.mode && p.mode !== 'auto' ? [{ text: p.mode, token: 'dim' as const }] : []),
        ],
      }, theme);
    },

    renderResult(result: unknown, _opts: unknown, theme?: PiTheme) {
      const r = result as { content?: Array<{ text?: string }> };
      const first = (r?.content?.[0]?.text ?? '').split('\n')[0] || 'callTool';
      // Match the codebase color contract: red=error, gold=act-on-me (blocked/
      // declined/proposal awaiting your decision), green=only a positive outcome.
      const state = first.startsWith('[ERROR]')
        ? 'error'
        : first.startsWith('[BLOCKED]') || first.startsWith('[DECLINED]') || first.startsWith('[PROPOSAL]')
          ? 'warning'
          : 'success';
      return buildToolView({ name: 'callTool', state, segments: [{ text: first, token: state === 'success' ? 'dim' : state }] }, theme);
    },
  });
}
