import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { StringDecoder } from 'node:string_decoder';
import fs from 'node:fs';
import path from 'node:path';
import { formatExternalAgentCoordinationContext } from '@octocodeai/octocode-awareness';
import { getInstallSource } from '../assets.js';
import { extensionTmpRoot } from '../extension-paths.js';
import { truncateUserVisibleToolOutput } from '../utils.js';
import { OCTOCODE_SPINNER_FRAMES } from '../ui-extras.js';
import { hasUiTickSubscriber, setUiTickSubscriber } from '../tui/ui-ticker.js';
import { shortId } from './ids.js';
import { SEP } from '../tui/palette.js';
import { openPersistentAwareness } from './storage-policy.js';
import type {
  PiContext,
  PiInstance,
  SpawnPolicy,
  SpawnPolicyResult,
  ToolCallResult,
  ToolDefinition,
  PiTheme,
  WorkerLedgerEntry,
  WorkerLedgerEvent,
  WorkerLedgerEventType,
  WorkerMessageActivity,
  WorkerWorktreeState,
} from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { cliToolTitle, paint } from '../tui/cli-design.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { refreshStatusPanel, resumeStatusPanel, setStatusPanelAgentSource } from './status-panel.js';
import { stringEnumSchema } from './schema-helpers.js';
import { setManagedStatus } from './runtime-renderer.js';
import { workspaceAgentRoot } from './session-artifacts.js';
import { getRandomAgentName } from '../agentNames.js';
import {
  assertWorktreeSpawnAllowed,
  cleanupWorktreeIfNoWork,
  createAgentWorktree,
  removeAgentWorktree,
  setWorktreeGitRunnerForTests,
  type InternalWorktreeState,
  type WorktreeGitRunner,
  type WorktreeIsolation,
} from './worktree.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

export type AgentStatus = 'starting' | 'running' | 'idle' | 'exited' | 'failed' | 'killed';
export type ResourceMode = 'lean' | 'octocode' | 'default';
export type NormalizedWorkerStatus = 'done' | 'blocked' | 'failed' | 'unknown';
export type NormalizedWorkerConfidence = 'confirmed' | 'likely' | 'uncertain';

type MessageAction = 'list' | 'status' | 'send' | 'steer' | 'followUp' | 'wait' | 'kill' | 'abort';
type WorktreeDecision = 'shared' | 'create';

export interface NormalizedWorkerResult {
  status: NormalizedWorkerStatus;
  result?: string;
  evidence: string[];
  verification?: string;
  confidence: NormalizedWorkerConfidence;
  next?: string;
  artifact?: string;
  rawPrefixes: Record<string, string[]>;
}

export interface WorkerRecoveryRisk {
  warnings: string[];
  statusOrActionCount: number;
  evidenceCount: number;
  hasVerification: boolean;
}

const REQUIRED_PACKET_SECTIONS = ['goal', 'context', 'scope', 'ownership', 'acceptance', 'return'];

type StreamHandler = (event: string, cb: (chunk: Buffer | string) => void) => void;
type ProcessHandler = (event: string, cb: (...args: unknown[]) => void) => void;

interface AgentProcess {
  stdin: { write(data: string): unknown; end?(): unknown };
  stdout: { on: StreamHandler };
  stderr: { on: StreamHandler };
  on: ProcessHandler;
  kill(signal?: NodeJS.Signals): boolean;
  killed?: boolean;
  /** null while running; a number once the process exited normally. */
  exitCode?: number | null;
  /** null while running; the signal name if the process was killed by a signal. */
  signalCode?: NodeJS.Signals | null;
}

interface SpawnOptions {
  cwd?: string;
  shell?: boolean;
  stdio?: Array<'ignore' | 'pipe'>;
  env?: NodeJS.ProcessEnv;
}

type AgentProcessFactory = (command: string, args: string[], options: SpawnOptions) => AgentProcess;
let ledgerHidden = false;

export interface SpawnAgentParams {
  task?: string;
  prompt?: string;
  context?: string;
  name?: string;
  cwd?: string;
  model?: string;
  /** Optional current plan step, shown in the parent agent ledger/footer. */
  planStep?: string;
  provider?: string;
  thinking?: string;
  tools?: string[];
  systemPrompt?: string;
  resourceMode?: ResourceMode;
  noSession?: boolean;
  isolation?: WorktreeIsolation;
  includeUncommitted?: boolean;
  /** Internal: set only after the user-decision gate approves worktree creation or explicitly chooses shared cwd. */
  worktreeDecision?: WorktreeDecision;
  /**
   * Absolute paths to skill directories to load via --skill (additive, works with --no-skills).
   * L9: This field is intentionally NOT exposed in the `spawnAgent` tool's TypeBox schema;
   * it is used internally by `spawnSubagent` which resolves skill directories from the
   * installed skill registry before calling `spawnRpcAgent`. Passing skills directly via
   * the `spawnAgent` tool params is unsupported and will be silently ignored by the schema
   * validator; use `spawnSubagent` instead.
   */
  skills?: string[];
}

interface AgentToolCall {
  toolCallId?: string;
  toolName: string;
  status: 'running' | 'done' | 'error';
  startedAt: number;
  finishedAt?: number;
  isError?: boolean;
}

interface AgentRecord {
  id: string;
  name: string;
  cwd: string;
  command: string;
  args: string[];
  task: string;
  planStep?: string;
  process: AgentProcess;
  status: AgentStatus;
  startedAt: number;
  updatedAt: number;
  exitCode?: number;
  signal?: string;
  error?: string;
  stderr: string;
  events: unknown[];
  messages: unknown[];
  responses: unknown[];
  toolCalls: AgentToolCall[];
  lastOutput: string;
  /** Rolling 1-line progress note (latest structured/progress line) shown live while the worker runs. */
  deltaSummary?: string;
  /** Durable markdown handback path assigned by the parent and safe to inspect after kill/remove. */
  handbackPath: string;
  /** Rolling 1-line summary of the worker's latest reasoning/thinking, distinct from output deltaSummary. */
  thinkingSummary?: string;
  /**
   * Count of messages queued to the worker (followUp / streaming send / idle steer)
   * that the worker has not yet begun a turn for. Cleared when the worker emits
   * agent_start. Keeps `wait` blocking and drives the 'queued' display state so the
   * ledger never shows 'running' before the turn actually starts.
   */
  pendingMessages: number;
  /** Latest directional parent↔worker communication for the footer ledger. */
  lastMessage?: WorkerMessageActivity;
  normalizedResult?: NormalizedWorkerResult;
  recoveryRisk: WorkerRecoveryRisk;
  ledgerEvents: WorkerLedgerEvent[];
  policyWarnings: string[];
  promptFiles: string[];
  waiters: Set<() => void>;
  /**
   * Heartbeat subscribers. Notified on every inbound RPC event (via touch) so a
   * blocking `wait` can reset its silence watchdog instead of enforcing a rigid
   * wall-clock deadline: as long as the worker streams events, the wait keeps going.
   */
  activityListeners: Set<() => void>;
  /**
   * In-flight liveness probes: request id → resolver. When the parent sends a
   * `get_state` probe during a silence gap, the child's correlated `response`
   * event resolves the matching entry, proving the worker is alive-but-quiet
   * rather than hung.
   */
  pendingProbes: Map<string, () => void>;
  nextRequestId: number;
  worktree?: InternalWorktreeState;
  /** Stable Awareness id used to register this worker in the shared agent list. */
  awarenessAgentId?: string;
  /** Workspace whose Awareness registry this worker joins (the parent workspace). */
  awarenessWorkspace?: string;
}

interface AgentDetails {
  agents: Array<ReturnType<typeof summarizeAgent>>;
}

const MAX_STORED_EVENTS = 200;
const MAX_LEDGER_EVENTS = 80;
const MAX_AGENT_VIEW_CHARS = 12_000;
const HANDBACK_ARTIFACT_FILENAME = 'handback.md';
/** Maximum number of simultaneously active (non-droppable) agent records. Hard limit enforced on spawn. */
export const MAX_AGENT_RECORDS = 50;
/** Cross-host root fan-out ceiling. Children never receive the agent facade. */
export const MAX_ACTIVE_AGENTS = 4;
export const DEFAULT_SPAWN_POLICY: SpawnPolicy = {
  maxActiveAgents: MAX_ACTIVE_AGENTS,
  warningActiveAgents: MAX_ACTIVE_AGENTS - 1,
  requiredPacketSections: REQUIRED_PACKET_SECTIONS,
  maxStepsPerWorker: 60,
};
const SPAWN_POLICY_MAX_ACTIVE_ENV = 'OCTOCODE_AGENT_MAX_ACTIVE';
const SPAWN_POLICY_WARNING_ACTIVE_ENV = 'OCTOCODE_AGENT_WARNING_ACTIVE';
const SPAWN_POLICY_MAX_STEPS_ENV = 'OCTOCODE_AGENT_MAX_STEPS';
/** Idle worker is reapable after this long with no update (backstop for orphaned agents). */
export const DEFAULT_IDLE_REAP_MS = 15 * 60_000;
export const OCTOCODE_AGENTS_COMMAND_USAGE = '/octocode-agents [help|list|status|inspect <id>|kill <id>|kill-all|prune|hide]';
export const OCTOCODE_AGENTS_COMMAND_COMPLETIONS = ['help', 'list', 'status', 'inspect ', 'kill ', 'kill-all', 'prune', 'hide'] as const;
export const OCTOCODE_AGENTS_COMMAND_DESCRIPTIONS: Record<(typeof OCTOCODE_AGENTS_COMMAND_COMPLETIONS)[number], string> = {
  help: 'Show command examples and lifecycle hints',
  list: 'Show the worker ledger and refresh footer/widget status',
  status: 'Alias for list',
  'inspect ': 'Show full state for one worker by id or prefix',
  'kill ': 'Stop one live worker by id or prefix',
  'kill-all': 'Stop every live worker',
  prune: 'Remove completed idle records from the in-memory ledger',
  hide: 'Clear the footer/widget ledger for this session',
};
const SUBAGENT_ENV_VAR = 'OCTOCODE_PI_SUBAGENT';
const AWARENESS_AGENT_ENV_VAR = 'OCTOCODE_AGENT_ID';
const FORBIDDEN_WORKER_TOOLS = new Set(['spawnAgent', 'AgentMessage', 'spawnSubagent']);
const agents = new Map<string, AgentRecord>();
const EXIT_SIGNALS: NodeJS.Signals[] = ['SIGTERM', 'SIGHUP', 'SIGINT'];
let processFactory: AgentProcessFactory = (command, args, options) => spawn(command, args, options) as unknown as AgentProcess;
let processCleanupHandlersInstalled = false;

/**
 * True when running inside a spawned worker process (marked via SUBAGENT_ENV_VAR).
 * Workers must not register any agent-spawning tool — recursive spawning is forbidden.
 */
export function isSubagentProcess(): boolean {
  return process.env[SUBAGENT_ENV_VAR] === '1';
}

export function setAgentProcessFactoryForTests(factory: AgentProcessFactory | null): void {
  processFactory = factory ?? ((command, args, options) => spawn(command, args, options) as unknown as AgentProcess);
  setWorktreeGitRunnerForTests(null);
  agents.clear();
  ledgerHidden = false;
  // A prior test's session_shutdown may have suppressed the status panel;
  // ledger rendering assertions need it live again.
  resumeStatusPanel();
}

export function setAgentWorktreeGitRunnerForTests(runner: WorktreeGitRunner | null): void {
  setWorktreeGitRunnerForTests(runner);
}

/** wait() resolves at end-of-turn: idle counts as "done for now", plus true terminals. */
function isTerminal(record: { status: AgentStatus; pendingMessages?: number }): boolean {
  // A worker with a queued-but-unstarted turn is not terminal: wait must keep
  // blocking and the display must not render it as idle/done.
  if ((record.pendingMessages ?? 0) > 0) return false;
  return ['idle', 'exited', 'failed', 'killed'].includes(record.status);
}

/**
 * Safe to drop from the registry WITHOUT killing: the child process is gone.
 * `idle` is NOT droppable — an idle worker's process is still alive to accept
 * send/steer/followUp, so evicting or shutdown-skipping it would orphan the child.
 */
function isDroppable(record: AgentRecord): boolean {
  if (record.worktree && record.worktree.mergeState !== 'clean' && record.worktree.mergeState !== 'merged' && record.worktree.mergeState !== 'discarded') return false;
  return ['exited', 'failed', 'killed'].includes(record.status);
}

/**
 * Whether the underlying OS process is still running (authoritative, sync).
 * A real ChildProcess reports null for both while running; test mocks may leave
 * them undefined — treat null/undefined (== null) as "still running".
 */
function isProcessAlive(record: AgentRecord): boolean {
  return record.process.exitCode == null && record.process.signalCode == null;
}

function evictStaleAgents(): void {
  if (agents.size <= MAX_AGENT_RECORDS) return;
  // Only evict records whose process is truly gone — never silently drop an
  // alive (running/idle/starting) worker, which would orphan the child process.
  const droppable = [...agents.entries()]
    .filter(([, r]) => isDroppable(r))
    .sort(([, a], [, b]) => a.updatedAt - b.updatedAt || a.startedAt - b.startedAt);
  while (agents.size > MAX_AGENT_RECORDS && droppable.length > 0) {
    const [id, record] = droppable.shift()!;
    removePromptFiles(record);
    agents.delete(id);
  }
}

/**
 * Drop every droppable (exited/failed/killed) agent record. Called on
 * session_start so a new session does not inherit dead worker rows from a
 * previous session in the same long-lived process. Alive/idle workers are
 * preserved — their process is still up.
 */
export function pruneDroppableAgentsForSession(): number {
  let removed = 0;
  for (const [id, record] of [...agents.entries()]) {
    if (!isDroppable(record)) continue;
    removePromptFiles(record);
    agents.delete(id);
    removed += 1;
  }
  return removed;
}

export function cleanupSpawnedAgentsForShutdown(): number {
  // Kill every worker whose process is still alive — including idle ones, whose
  // process stays up between turns and would otherwise survive as an orphan.
  const alive = [...agents.values()].filter((record) => isProcessAlive(record));
  for (const record of alive) killAgent(record, { forceKillDelayMs: 0 });
  // The killed children's close/stderr events fire on later ticks and call
  // refreshAgentLedgerUi; hide the ledger so those callbacks clear rather than
  // resurrect the status/widget into the next session. Spawning (or an explicit
  // list/status action) un-hides it again.
  ledgerHidden = true;
  stopLedgerTicker();
  return alive.length;
}

function installProcessCleanupHandlers(): void {
  if (processCleanupHandlersInstalled || process.env[SUBAGENT_ENV_VAR] === '1') return;
  processCleanupHandlersInstalled = true;
  const cleanup = () => { cleanupSpawnedAgentsForShutdown(); };
  process.once('beforeExit', cleanup);
  process.once('exit', cleanup);
  for (const signal of EXIT_SIGNALS) {
    process.once(signal, () => {
      cleanup();
      process.kill(process.pid, signal);
    });
  }
}



// ─── TUI rendering helpers ────────────────────────────────────────────────────
// truncateToWidth + makeRenderer imported from render-helpers.ts (single source)

type AgentDisplayState = 'starting' | 'queued' | 'running' | 'idle' | 'done' | 'blocked' | 'failed' | 'killed';

type AgentDisplaySource = {
  status?: string;
  pendingMessages?: number;
  normalizedResult?: { status?: string; result?: string; next?: string; confidence?: string; verification?: string };
};

function getAgentDisplayState(agent: AgentDisplaySource): AgentDisplayState {
  const workerStatus = agent.normalizedResult?.status;
  if (agent.status === 'killed') return 'killed';
  if (agent.status === 'failed' || workerStatus === 'failed') return 'failed';
  if (agent.status === 'running') return 'running';
  // A queued-but-unstarted turn takes precedence over an idle/done snapshot so the
  // ledger never shows 'running' before agent_start, nor 'done' with work pending.
  if ((agent.pendingMessages ?? 0) > 0) return 'queued';
  // Exited beats blocked: a dead process that last said [BLOCKED] cannot be
  // steered/unblocked, so showing "blocked" would advertise a dead-end action.
  if (agent.status === 'exited') return 'done';
  if (workerStatus === 'blocked') return 'blocked';
  if (workerStatus === 'done') return 'done';
  if (agent.status === 'idle') return 'idle';
  return 'starting';
}

// Live-progress spinner: advanced once per ledger tick while a worker runs.
// Shares the working-indicator frames so the extension has ONE spinner glyph
// set (the ledger just steps it at the panel's 1s cadence).
const LEDGER_SPINNER = OCTOCODE_SPINNER_FRAMES;
let ledgerSpinnerFrame = 0;
/** Shared-clock subscription key; live only while ≥1 worker is non-terminal and the UI is present. */
const LEDGER_TICK_KEY = 'octocode-ledger';
let agentLedgerMetricsRefresh: ((ctx?: PiContext) => void) | undefined;

function stopLedgerTicker(): void {
  setUiTickSubscriber(LEDGER_TICK_KEY, undefined);
}

/** Test hook: stop the ticker and report its state so tests never leak a real timer. */
export function stopLedgerTickerForTests(): void {
  stopLedgerTicker();
}
export function isLedgerTickerActiveForTests(): boolean {
  return hasUiTickSubscriber(LEDGER_TICK_KEY);
}

/**
 * Expanded agent tool-result body: the header line plus the dim output lines
 * after the two-line agent-header/status preamble. Shared by the spawnAgent
 * and AgentMessage renderers.
 */
function renderExpandedAgentResult(header: string, result: ToolCallResult, theme?: PiTheme) {
  const text = result.content.find((p) => p.type === 'text')?.text ?? '';
  const preview = truncateUserVisibleToolOutput(text, MAX_AGENT_VIEW_CHARS);
  const allLines = preview.text.split('\n');
  // Line 0 of the result text is always the plain-text agent identifier line
  // (e.g. "AgentMessage action:wait [my-agent]") produced by renderSingleAgentResult.
  // The styled `header` arg already occupies the first rendered row, so we skip
  // exactly one line — not two — so structured fields like agentId: remain visible
  // in expanded mode. A fixed offset of 1 is safe: the caller always controls the
  // content format via renderSingleAgentResult, which always puts the identifier
  // on line 0 and the first structured field (agentId) on line 1.
  const outputLines = allLines.slice(1);
  if (preview.truncated) {
    outputLines.push(`… ${preview.omittedChars} chars hidden in the UI; the complete agent result remains in tool context`);
  }
  return makeRenderer((w) => [
    truncateToWidth(header, w),
    ...outputLines.map((l) => truncateToWidth(paint(theme, 'dim', l), w)),
  ]);
}

// `frozen` pins the running glyph to a static frame: the live ledger (below the
// editor) may animate, but a persisted transcript result entry must be a PURE
// function of its captured state \u2014 sampling the moving ledgerSpinnerFrame there
// would repaint the row on every tick and make the scrollback jump.
function agentDisplayMeta(state: AgentDisplayState, theme?: PiTheme, opts: { frozen?: boolean } = {}): { icon: string; label: string } {
  const raw: { icon: string; label: string; color: Parameters<typeof paint>[1] } = (() => {
    switch (state) {
      case 'done': return { icon: '\u2713', label: 'done', color: 'success' };
      case 'failed': return { icon: '\u2717', label: 'failed', color: 'error' };
      // killed is a neutral terminal state (dismissed), not act-on-me \u2014 gold is
      // reserved for blocked (the row you must act on). Muted so the two differ.
      case 'killed': return { icon: '\u2717', label: 'killed', color: 'muted' };
      case 'blocked': return { icon: '!', label: 'blocked', color: 'warning' };
      // Running is normal activity (brand), idle is quiet (muted) \u2014 warning and
      // success stay reserved for real attention/outcome states.
      case 'running': return { icon: opts.frozen ? LEDGER_SPINNER[0]! : LEDGER_SPINNER[ledgerSpinnerFrame % LEDGER_SPINNER.length], label: 'running', color: 'brand' };
      case 'idle': return { icon: '\u25CE', label: 'idle', color: 'muted' };
      case 'queued': return { icon: '\u21e5', label: 'queued', color: 'link' };
      case 'starting': return { icon: '\u25CB', label: 'starting', color: 'dim' };
    }
  })();
  return {
    icon: paint(theme, raw.color, raw.icon),
    label: paint(theme, raw.color, raw.label),
  };
}

function statusIcon(status: AgentStatus, theme?: PiTheme): string {
  return agentDisplayMeta(getAgentDisplayState({ status }), theme).icon;
}


/**
 * `endedAt` freezes elapsed time at a terminal agent's last update instead of
 * letting it keep growing against Date.now() long after the agent finished —
 * pass it whenever the record/summary is terminal (see isTerminal()).
 */
export function formatElapsed(startedAt: number, endedAt?: number): string {
  const ms = (endedAt ?? Date.now()) - startedAt;
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${Math.round(ms / 1000)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return s > 0 ? `${m}m${s}s` : `${m}m`;
}

function getPiInvocation(args: string[]): { command: string; args: string[] } {
  const currentScript = process.argv[1];
  const isBunVirtualScript = currentScript?.startsWith('/$bunfs/root/');
  if (currentScript && !isBunVirtualScript && fs.existsSync(currentScript)) {
    return { command: process.execPath, args: [currentScript, ...args] };
  }

  const execName = path.basename(process.execPath).toLowerCase();
  const isGenericRuntime = /^(node|bun)(\.exe)?$/.test(execName);
  if (!isGenericRuntime) return { command: process.execPath, args };

  return { command: 'pi', args };
}

function safeName(value: string): string {
  return value.replace(/[^\w.-]+/g, '_').slice(0, 80) || 'agent';
}

function writeTempPromptFile(name: string, text: string): string {
  const root = path.join(extensionTmpRoot(), 'prompts');
  fs.mkdirSync(root, { recursive: true, mode: 0o700 });
  const dir = fs.mkdtempSync(path.join(root, 'worker-'));
  const filePath = path.join(dir, `${safeName(name)}.md`);
  fs.writeFileSync(filePath, text, { encoding: 'utf8', mode: 0o600 });
  return filePath;
}

function buildHandbackPath(workspace: string, agentId: string): string {
  return path.join(workspaceAgentRoot(workspace), 'workers', agentId, HANDBACK_ARTIFACT_FILENAME);
}

function ensureHandbackDir(filePath: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true, mode: 0o700 });
}

function prepareHandbackPath(workspace: string, agentId: string): { path: string; warning?: string } {
  const preferredPath = buildHandbackPath(workspace, agentId);
  try {
    ensureHandbackDir(preferredPath);
    return { path: preferredPath };
  } catch (preferredError) {
    const fallbackPath = path.join(
      extensionTmpRoot(),
      'handbacks',
      agentId,
      HANDBACK_ARTIFACT_FILENAME,
    );
    try {
      ensureHandbackDir(fallbackPath);
    } catch (fallbackError) {
      const preferredMessage = preferredError instanceof Error ? preferredError.message : String(preferredError);
      const fallbackMessage = fallbackError instanceof Error ? fallbackError.message : String(fallbackError);
      throw new Error(`Unable to prepare worker handback directory (${preferredMessage}); fallback also failed (${fallbackMessage}).`);
    }
    const preferredMessage = preferredError instanceof Error ? preferredError.message : String(preferredError);
    return {
      path: fallbackPath,
      warning: `Parent workspace handback directory is unavailable; using temporary fallback ${fallbackPath} (${preferredMessage}).`,
    };
  }
}

function statHandbackArtifact(filePath: string): { path: string; exists: boolean; bytes?: number; modifiedAt?: string } {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return { path: filePath, exists: false };
    return { path: filePath, exists: true, bytes: stat.size, modifiedAt: stat.mtime.toISOString() };
  } catch {
    return { path: filePath, exists: false };
  }
}

function removePromptFiles(record: AgentRecord): void {
  for (const filePath of record.promptFiles) {
    try {
      fs.rmSync(path.dirname(filePath), { recursive: true, force: true });
    } catch {
      // best-effort cleanup only
    }
  }
  record.promptFiles = [];
}

function buildInitialPrompt(params: SpawnAgentParams): string {
  const task = String(params.task ?? params.prompt ?? '').trim();
  const context = String(params.context ?? '').trim();
  if (!context) return task;
  return `Context for this delegated agent:\n\n${context}\n\nTask:\n\n${task}`;
}

/**
 * Append an Awareness coordination footer so the worker knows its own durable id
 * and peer ids. The package owns usage policy; Pi adds only its handback path.
 */
export function withPeerCoordination(
  task: string,
  selfId: string | undefined,
  peerIds: string[],
  opts: { parentId?: string; handbackPath?: string } = {},
): string {
  if (!selfId) return task;
  const coordination = formatExternalAgentCoordinationContext({ selfId, parentId: opts.parentId, peerIds });
  const lines = [
    coordination,
    opts.handbackPath ? `- durable handback file: ${opts.handbackPath}` : undefined,
    opts.handbackPath ? '- before a terminal [DONE]/[BLOCKED]/[FAILED] when findings are long or important, write concise Markdown to that exact file (Status, Result, Evidence, Verification, Next), then include `[ARTIFACT] <path>` in your final output.' : undefined,
  ].filter((line): line is string => Boolean(line));
  return `${task}\n\n${lines.join('\n')}`;
}

/** Best-effort, advisory: register/deregister a worker in the shared Awareness agent list. Never throws. */
function syncWorkerRegistry(action: 'join' | 'leave', record: AgentRecord): void {
  const agentId = record.awarenessAgentId;
  const workspace = record.awarenessWorkspace;
  if (!agentId || !workspace) return;
  let aw: ReturnType<typeof openPersistentAwareness> | undefined;
  try {
    aw = openPersistentAwareness({ workspace });
    if (action === 'join') aw.joinAgent({ agentId, name: record.name, role: 'worker' });
    else aw.leaveAgent({ agentId });
  } catch { /* Awareness unresolved — advisory */ }
  finally { aw?.close(); }
}

/** Awareness ids of other still-alive workers, for peer-messaging discovery. */
function collectPeerAwarenessIds(excludeId: string): string[] {
  const ids: string[] = [];
  for (const rec of agents.values()) {
    if (rec.id === excludeId || !rec.awarenessAgentId) continue;
    if (rec.status !== 'exited' && rec.status !== 'failed' && rec.status !== 'killed') ids.push(rec.awarenessAgentId);
  }
  return ids;
}

function withWorktreePromptContext(params: SpawnAgentParams, worktree: InternalWorktreeState): SpawnAgentParams {
  const preamble = [
    'Worktree isolation is active for this worker.',
    `- Worktree path: ${worktree.path}`,
    `- Branch: ${worktree.branch}`,
    `- Base commit: ${worktree.baseCommit}`,
    '- Report repo-relative paths in handback; the parent ledger exposes the isolated path for review.',
  ].join('\n');
  return { ...params, cwd: worktree.path, context: params.context ? `${preamble}\n\n${params.context}` : preamble };
}

function getWorkerTools(params: SpawnAgentParams): string[] {
  return (params.tools ?? []).filter((toolName) => !FORBIDDEN_WORKER_TOOLS.has(toolName));
}

function buildPiArgs(params: SpawnAgentParams, name: string, promptFiles: string[]): string[] {
  const resourceMode = params.resourceMode ?? 'lean';
  const args = ['--mode', 'rpc'];
  const workerTools = getWorkerTools(params);

  if (params.noSession !== false) args.push('--no-session');
  // Load specific skills even when --no-skills is active (additive)
  for (const skillPath of params.skills ?? []) args.push('--skill', skillPath);
  args.push('--name', name);
  args.push('--exclude-tools', [...FORBIDDEN_WORKER_TOOLS].join(','));

  if (params.provider) args.push('--provider', params.provider);
  if (params.model) args.push('--model', params.model);
  if (shouldForceThinkingOffForToolCallingWorker(params, workerTools)) args.push('--thinking', 'off');
  else if (params.thinking) args.push('--thinking', params.thinking);
  if (workerTools.length) args.push('--tools', workerTools.join(','));
  args.push('--no-context-files');

  if (resourceMode === 'lean') {
    args.push('--no-extensions', '--no-skills', '--no-prompt-templates', '--no-themes');
  } else if (resourceMode === 'octocode') {
    args.push('--no-extensions', '-e', getInstallSource(), '--no-skills', '--no-prompt-templates', '--no-themes');
  }

  const systemPrompt = String(params.systemPrompt ?? '').trim();
  if (systemPrompt) {
    const filePath = writeTempPromptFile(name, systemPrompt);
    promptFiles.push(filePath);
    args.push('--append-system-prompt', filePath);
  }

  return args;
}

function touch(record: AgentRecord, status?: AgentStatus): void {
  record.updatedAt = Date.now();
  if (status) record.status = status;
  // Every touch is driven by an inbound RPC event (tool call, output delta, turn
  // boundary, …) — i.e. proof the worker is alive and progressing. Fan it out to
  // any blocking waiter so it can reset its silence watchdog. A throwing listener
  // must never break the event pipeline.
  if (record.activityListeners.size > 0) {
    for (const listener of record.activityListeners) {
      try { listener(); } catch { /* listener errors are isolated */ }
    }
  }
}

/**
 * Mark that a turn has been queued to the worker but has not started yet. Bumps
 * pendingMessages (drives the 'queued' display state and keeps `wait` blocking)
 * and refreshes the timestamp without faking a 'running' status.
 */
function enqueueWorkerTurn(record: AgentRecord): void {
  record.pendingMessages += 1;
  touch(record);
}

// Ledger listeners: notified on every ledger event (spawned/status/tool/handback/…).
// Normalized-status flips also funnel through here — refreshNormalizedResult pushes a
// 'handback' ledger event whenever the normalized status changes, so subscribing to
// pushLedgerEvent covers all worker state transitions.
const ledgerListeners = new Set<(entry: WorkerLedgerEntry, type: WorkerLedgerEventType) => void>();

/** Subscribe to worker ledger events. Returns an unsubscribe function. */
export function registerWorkerLedgerListener(cb: (entry: WorkerLedgerEntry, type: WorkerLedgerEventType) => void): () => void {
  ledgerListeners.add(cb);
  return () => { ledgerListeners.delete(cb); };
}

function pushLedgerEvent(record: AgentRecord, type: WorkerLedgerEventType, message?: string, details?: unknown): void {
  pushCapped(record.ledgerEvents, {
    type,
    timestamp: Date.now(),
    message,
    details,
  });
  if (record.ledgerEvents.length > MAX_LEDGER_EVENTS) record.ledgerEvents.splice(0, record.ledgerEvents.length - MAX_LEDGER_EVENTS);
  if (ledgerListeners.size > 0) {
    const entry = toWorkerLedgerEntry(record);
    for (const listener of ledgerListeners) {
      // A throwing listener must never break the ledger (or the worker pipeline).
      try { listener(entry, type); } catch { /* listener errors are isolated */ }
    }
  }
}

function previewMessage(message: string): string {
  const oneLine = message.replace(/\s+/g, ' ').trim();
  return oneLine.length > 72 ? `${oneLine.slice(0, 71)}…` : oneLine;
}

function recordMessageActivity(
  record: AgentRecord,
  direction: WorkerMessageActivity['direction'],
  action: WorkerMessageActivity['action'],
  message: string,
  ledgerMessage: string,
): void {
  record.lastMessage = {
    direction,
    action,
    preview: previewMessage(message),
    timestamp: Date.now(),
  };
  pushLedgerEvent(record, 'message', ledgerMessage, record.lastMessage);
}

function recordInboundMessage(record: AgentRecord, message: unknown): void {
  if (!isAssistantOutputMessage(message)) return;
  const text = extractTextFromMessage(message);
  if (!text) return;
  recordMessageActivity(record, 'from-agent', 'reply', text, `reply received: ${previewMessage(text)}`);
}

function notifyWaiters(record: AgentRecord): void {
  for (const waiter of record.waiters) waiter();
  record.waiters.clear();
}

function pushCapped<T>(items: T[], item: T): void {
  items.push(item);
  if (items.length > MAX_STORED_EVENTS) items.splice(0, items.length - MAX_STORED_EVENTS);
}

function activeAgentCount(): number {
  return [...agents.values()].filter((record) => !isDroppable(record)).length;
}

/**
 * A packet section counts as present only when it anchors a line as a label —
 * e.g. "Goal:", "- Scope:", "## Ownership —", "**Acceptance:**" — optionally
 * preceded by a bullet/heading marker and wrapped in bold. A bare mention
 * inside prose ("there is no clear goal here") does not count: the packet
 * policy exists to catch genuinely unstructured worker briefs, not to be
 * satisfied by incidentally using the right words.
 */
function missingStructuredSections(text: string, sections: string[]): string[] {
  return sections.filter((section) => {
    const label = new RegExp(String.raw`^[ \t]*(?:[-*#>]+[ \t]*)*\**${section}\**[ \t]*[:—-]`, 'im');
    return !label.test(text);
  });
}

function readPositiveIntegerEnv(name: string): number | undefined {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === '') return undefined;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0) return undefined;
  return value;
}

function resolveSpawnPolicy(policy: SpawnPolicy): SpawnPolicy {
  const maxActiveAgents = Math.min(
    readPositiveIntegerEnv(SPAWN_POLICY_MAX_ACTIVE_ENV) ?? policy.maxActiveAgents,
    MAX_ACTIVE_AGENTS,
  );
  const warningActiveAgents = readPositiveIntegerEnv(SPAWN_POLICY_WARNING_ACTIVE_ENV) ?? policy.warningActiveAgents;
  const maxStepsPerWorker = readPositiveIntegerEnv(SPAWN_POLICY_MAX_STEPS_ENV) ?? policy.maxStepsPerWorker;
  return {
    ...policy,
    maxActiveAgents,
    warningActiveAgents: Math.min(warningActiveAgents, maxActiveAgents),
    maxStepsPerWorker,
  };
}

/**
 * Per-worker step (tool-call) circuit-breaker signal. Returns a warning once a worker's
 * completed tool calls reach the budget so the parent can abort/steer a runaway worker.
 */
export function evaluateStepBudget(
  steps: number,
  maxSteps: number = DEFAULT_SPAWN_POLICY.maxStepsPerWorker,
): { exceeded: boolean; warning?: string } {
  if (!Number.isFinite(maxSteps) || maxSteps <= 0) return { exceeded: false };
  if (steps >= maxSteps) {
    return { exceeded: true, warning: `Worker exceeded step budget (${steps}/${maxSteps} tool calls) — consider abort/steer; a runaway worker burns tokens.` };
  }
  return { exceeded: false };
}

/**
 * Classify lingering agents for cleanup. TERMINAL records (exited/failed/killed) are always
 * safe to auto-remove; IDLE records are still reusable for follow-ups, so they are only
 * flagged (nudge) once idle longer than idleMs — never auto-killed (preserves the follow-up
 * model). Pure + injectable clock for testing.
 */
export function findReapableIdleAgents(
  records: Array<{ id: string; status: AgentStatus; updatedAt: number }>,
  opts: { idleMs?: number; now?: number } = {},
): { terminal: string[]; idle: string[] } {
  const idleMs = opts.idleMs ?? DEFAULT_IDLE_REAP_MS;
  const now = opts.now ?? Date.now();
  const terminalStatuses: AgentStatus[] = ['exited', 'failed', 'killed'];
  const terminal: string[] = [];
  const idle: string[] = [];
  for (const r of records) {
    const staleFor = now - r.updatedAt;
    if (terminalStatuses.includes(r.status)) terminal.push(r.id);
    else if (r.status === 'idle' && staleFor >= idleMs) idle.push(r.id);
  }
  return { terminal, idle };
}

function looksLikeProviderScopedModel(model: string): boolean {
  return /\//.test(model)
    || /^(?:claude|gpt|llama|mistral|gemini|qwen|zai|deepseek|kimi|codestral)[-_:/.]/i.test(model);
}

function isOpenAiGpt5Worker(params: SpawnAgentParams): boolean {
  const provider = String(params.provider ?? '').toLowerCase();
  const model = String(params.model ?? '').toLowerCase();
  return provider.includes('openai') && /^gpt-5(?:[._-]|$)/.test(model);
}

function shouldForceThinkingOffForToolCallingWorker(params: SpawnAgentParams, workerTools: string[]): boolean {
  // OpenAI's Chat Completions endpoint rejects function tools when reasoning_effort
  // is present for GPT-5-series models. Omitting --thinking can inherit a parent or
  // configured default, so tool-calling subagents must explicitly disable it.
  return workerTools.length > 0 && isOpenAiGpt5Worker(params);
}

function resolveWorkerModelParams(params: SpawnAgentParams, ctx?: PiContext): SpawnAgentParams {
  const explicitModel = typeof params.model === 'string' && params.model.trim().length > 0;
  const parentModel = ctx?.model;
  return {
    ...params,
    model: explicitModel ? params.model : parentModel?.id,
    provider: params.provider ?? (!explicitModel || params.model === parentModel?.id ? parentModel?.provider : undefined),
  };
}

function validateWorkerModelParams(params: SpawnAgentParams, ctx?: PiContext): void {
  const model = String(params.model ?? '').trim();
  const provider = String(params.provider ?? '').trim();
  if (!model) return;
  if (!provider && looksLikeProviderScopedModel(model)) {
    throw new Error(`spawnAgent model "${model}" requires an explicit provider from \`pi -ne --list-models\`.`);
  }
  if (provider && ctx?.modelRegistry?.find && !ctx.modelRegistry.find(provider, model)) {
    throw new Error(`spawnAgent model/provider not found in the active Pi model registry: ${provider}/${model}. Choose a valid pair from \`pi -ne --list-models\`.`);
  }
}

export function evaluateSpawnPolicy(params: SpawnAgentParams, activeCount = activeAgentCount(), policy: SpawnPolicy = DEFAULT_SPAWN_POLICY): SpawnPolicyResult {
  const effectivePolicy = resolveSpawnPolicy(policy);
  const warnings: string[] = [];
  if (params.isolation === 'worktree') {
    try {
      assertWorktreeSpawnAllowed(path.resolve(String(params.cwd ?? process.cwd())));
    } catch (error) {
      return {
        allowed: false,
        warnings,
        reason: error instanceof Error ? error.message : String(error),
      };
    }
  }
  if (activeCount >= effectivePolicy.maxActiveAgents) {
    return {
      allowed: false,
      warnings,
      reason: `Agent registry at capacity: ${activeCount}/${effectivePolicy.maxActiveAgents} active agents.`,
    };
  }
  if (activeCount >= effectivePolicy.warningActiveAgents) {
    warnings.push(`High worker fan-out: ${activeCount}/${effectivePolicy.maxActiveAgents} active agents already exist.`);
  }
  const task = buildInitialPrompt(params);
  const missingSections = missingStructuredSections(task, effectivePolicy.requiredPacketSections);
  if (missingSections.length > 0) {
    warnings.push(`Worker packet is missing recommended section(s): ${missingSections.join(', ')}.`);
  }
  const model = String(params.model ?? '');
  if (model && looksLikeProviderScopedModel(model) && !params.provider) {
    warnings.push('Model looks provider-scoped or custom-provider-hosted; pass provider from `pi -ne --list-models` when required.');
  }
  if (shouldForceThinkingOffForToolCallingWorker(params, getWorkerTools(params))) {
    warnings.push('Forced --thinking off for OpenAI GPT-5 tool-calling worker because Chat Completions rejects reasoning_effort with function tools.');
  }
  const strippedTools = (params.tools ?? []).filter((toolName) => FORBIDDEN_WORKER_TOOLS.has(toolName));
  if (strippedTools.length > 0) {
    warnings.push(`Recursive worker tool(s) stripped: ${strippedTools.join(', ')}.`);
  }
  return { allowed: true, warnings };
}

function extractTextFromMessage(message: unknown): string {
  const content = (message as { content?: unknown })?.content;
  if (!Array.isArray(content)) return '';
  return content
    .map((part) => ((part as { type?: string; text?: string }).type === 'text' ? (part as { text?: string }).text ?? '' : ''))
    .filter(Boolean)
    .join('\n');
}

/** Concatenated `thinking` content parts of an assistant message (session-format ThinkingContent). */
function extractThinkingFromMessage(message: unknown): string {
  const content = (message as { content?: unknown })?.content;
  if (!Array.isArray(content)) return '';
  return content
    .map((part) => ((part as { type?: string; thinking?: string }).type === 'thinking' ? (part as { thinking?: string }).thinking ?? '' : ''))
    .filter(Boolean)
    .join('\n');
}

function normalizeConfidence(value: string | undefined): NormalizedWorkerConfidence {
  const lower = String(value ?? '').toLowerCase();
  if (lower.includes('confirmed')) return 'confirmed';
  if (lower.includes('likely')) return 'likely';
  return 'uncertain';
}

export function normalizeWorkerOutput(output: string): NormalizedWorkerResult {
  const rawPrefixes: Record<string, string[]> = {};
  const lines = output.split(/\r?\n/);
  for (const line of lines) {
    const match = line.match(/^\s*\[([A-Z][A-Z0-9_-]*)\]\s*(.*)$/);
    if (!match) continue;
    const prefix = match[1]!;
    (rawPrefixes[prefix] ??= []).push(match[2]!.trim());
  }

  const last = (prefix: string): string | undefined => rawPrefixes[prefix]?.at(-1);
  const evidence = rawPrefixes['EVIDENCE'] ?? [];
  const blocked = last('BLOCKED');
  const done = last('DONE');
  const failed = last('FAILED') ?? last('ERROR');
  // Cover every salient prefix the subagent SYSTEM_PROMPTs solicit — a worker
  // whose best output is [RISK]/[IMPACT]/[GAP] must still surface it in the
  // normalized handback instead of silently dropping to result:undefined.
  const result =
    last('RESULT')
    ?? last('FINDING')
    ?? last('ROOT')
    ?? last('FIX')
    ?? last('PLAN')
    ?? last('ACTION')
    ?? last('IMPACT')
    ?? last('RISK')
    ?? last('GAP')
    ?? last('ASSUMPTION')
    ?? last('QUERY')
    ?? last('METRIC')
    ?? undefined;
  const verification = last('VERIFICATION') ?? last('VERIFY') ?? undefined;
  const artifact = last('ARTIFACT') ?? last('HANDOFF') ?? undefined;
  const fallback = output.trim();

  return {
    status: failed ? 'failed' : blocked ? 'blocked' : done ? 'done' : 'unknown',
    result: result || (Object.keys(rawPrefixes).length === 0 && fallback ? fallback : undefined),
    evidence,
    verification,
    confidence: normalizeConfidence(last('CONFIDENCE')),
    next: last('NEXT') || blocked || done || undefined,
    artifact,
    rawPrefixes,
  };
}

export function evaluateWorkerRecoveryRisk(output: string): WorkerRecoveryRisk {
  const normalized = normalizeWorkerOutput(output);
  const statusOrActionCount = (normalized.rawPrefixes['STATUS']?.length ?? 0)
    + (normalized.rawPrefixes['ACTION']?.length ?? 0)
    + (normalized.rawPrefixes['FIX']?.length ?? 0);
  const evidenceCount = normalized.evidence.length;
  const hasVerification = Boolean(normalized.verification);
  const warnings: string[] = [];

  if (statusOrActionCount >= 4 && evidenceCount === 0 && !hasVerification) {
    warnings.push(
      `Possible recovery loop: ${statusOrActionCount} status/action updates without evidence or verification; re-diagnose before continuing.`,
    );
  }

  if (normalized.status === 'done' && evidenceCount === 0 && !hasVerification) {
    warnings.push('Worker claims done without evidence or verification; parent must independently verify acceptance.');
  }

  return { warnings, statusOrActionCount, evidenceCount, hasVerification };
}

function refreshNormalizedResult(record: AgentRecord): void {
  const previousStatus = record.normalizedResult?.status;
  const previousWarnings = record.recoveryRisk.warnings.join('\n');
  const output = record.lastOutput || record.stderr || record.error || '';
  record.normalizedResult = normalizeWorkerOutput(output);
  record.recoveryRisk = evaluateWorkerRecoveryRisk(output);
  // Step-budget circuit-breaker: surface a warning when a worker's completed tool calls
  // reach the budget, so the parent can abort/steer a runaway worker.
  const steps = record.toolCalls.filter((call) => call.status !== 'running').length;
  const budget = evaluateStepBudget(steps, resolveSpawnPolicy(DEFAULT_SPAWN_POLICY).maxStepsPerWorker);
  if (budget.exceeded && budget.warning && !record.recoveryRisk.warnings.includes(budget.warning)) {
    record.recoveryRisk.warnings.push(budget.warning);
  }
  if (record.normalizedResult.status !== 'unknown' && record.normalizedResult.status !== previousStatus) {
    pushLedgerEvent(record, 'handback', `worker handback: ${record.normalizedResult.status}`, record.normalizedResult);
  }
  const currentWarnings = record.recoveryRisk.warnings.join('\n');
  if (currentWarnings && currentWarnings !== previousWarnings) {
    pushLedgerEvent(record, 'policy', `recovery risk: ${record.recoveryRisk.warnings.join(' | ')}`, record.recoveryRisk);
  }
}

const DELTA_PREFIX = /^\s*\[(STATUS|ACTION|FINDING|METRIC|PLAN|BLOCKED|DONE|EVIDENCE)\]/i;
const MAX_DELTA_SUMMARY_CHARS = 120;

/** Rolling progress note: latest structured worker line, else the last non-empty line. */
export function extractDeltaSummary(text: string): string | undefined {
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);
  if (lines.length === 0) return undefined;
  const structured = [...lines].reverse().find((l) => DELTA_PREFIX.test(l));
  const chosen = (structured ?? lines[lines.length - 1]!).replace(/\s+/g, ' ');
  return chosen.length > MAX_DELTA_SUMMARY_CHARS ? `${chosen.slice(0, MAX_DELTA_SUMMARY_CHARS - 1)}…` : chosen;
}

/**
 * SEV-2: capture a model/turn-level failure from a worker message. Errored turns emit
 * an assistant message with stopReason:"error" and errorMessage (e.g. "400 Unsupported
 * model", "500 Internal Server Error") but no content, so without this the failure is
 * invisible: record.error stays unset (RPC transport succeeded, process exits 0) and
 * lastOutput is empty. Recording it lets status/AgentMessage explain why a worker idled.
 */
function captureMessageError(record: AgentRecord, message: unknown): void {
  if (!message || typeof message !== 'object') return;
  const m = message as { stopReason?: string; errorMessage?: string };
  const errMsg = typeof m.errorMessage === 'string' ? m.errorMessage.trim() : '';
  if (m.stopReason !== 'error' && !errMsg) return;
  const text = errMsg || 'worker model turn failed';
  if (!record.error) record.error = text;
  pushLedgerEvent(record, 'error', `worker turn error: ${text}`);
  touch(record);
}

function isAssistantOutputMessage(message: unknown): boolean {
  if (!message || typeof message !== 'object') return false;
  return (message as { role?: unknown }).role === 'assistant';
}

function updateLastOutput(record: AgentRecord, message: unknown): void {
  if (!isAssistantOutputMessage(message)) return;
  const text = extractTextFromMessage(message);
  if (text) {
    record.lastOutput = text;
    const delta = extractDeltaSummary(text);
    if (delta) record.deltaSummary = delta;
    refreshNormalizedResult(record);
  }
  const thinking = extractThinkingFromMessage(message);
  if (thinking) {
    // Keep only the last non-empty reasoning line as a rolling one-line summary.
    const lastLine = thinking.split('\n').map((l) => l.trim()).filter(Boolean).at(-1);
    if (lastLine) record.thinkingSummary = lastLine;
  }
}

function getEventToolName(event: Record<string, unknown>): string {
  return String(event['toolName'] ?? event['tool_name'] ?? event['tool'] ?? event['name'] ?? '').trim();
}

function getEventToolCallId(event: Record<string, unknown>): string | undefined {
  const id = event['toolCallId'] ?? event['tool_call_id'] ?? event['id'];
  return typeof id === 'string' && id.trim() ? id : undefined;
}

function recordToolStart(record: AgentRecord, event: Record<string, unknown>): void {
  const toolName = getEventToolName(event);
  if (!toolName) return;
  pushCapped(record.toolCalls, {
    toolCallId: getEventToolCallId(event),
    toolName,
    status: 'running',
    startedAt: Date.now(),
  });
  pushLedgerEvent(record, 'tool', `tool started: ${toolName}`);
  touch(record, 'running');
}

function recordToolEnd(record: AgentRecord, event: Record<string, unknown>): void {
  const toolName = getEventToolName(event);
  const toolCallId = getEventToolCallId(event);
  if (!toolName && !toolCallId) return;
  const call = [...record.toolCalls].reverse().find((item) => (
    toolCallId ? item.toolCallId === toolCallId : item.toolName === toolName
  ) && item.status === 'running');
  const isError = Boolean(event['isError'] ?? event['is_error'] ?? event['error']);
  if (call) {
    call.status = isError ? 'error' : 'done';
    call.finishedAt = Date.now();
    call.isError = isError;
  } else if (toolName) {
    pushCapped(record.toolCalls, {
      toolCallId,
      toolName,
      status: isError ? 'error' : 'done',
      startedAt: Date.now(),
      finishedAt: Date.now(),
      isError,
    });
  }
  if (toolName) pushLedgerEvent(record, 'tool', `tool ${isError ? 'failed' : 'finished'}: ${toolName}`);
  touch(record);
}

function formatToolCalls(toolCalls: AgentToolCall[], limit = 3): string {
  const recent = toolCalls.slice(-limit);
  return recent.map((call) => `${call.toolName}:${call.status}`).join(', ');
}

function processRpcLine(record: AgentRecord, line: string): void {
  if (!line.trim()) return;
  let event: unknown;
  try {
    event = JSON.parse(line);
  } catch {
    return;
  }

  pushCapped(record.events, event);
  const eventObject = event as Record<string, unknown>;
  const eventType = (event as { type?: string }).type;
  if (eventType === 'tool_call' || eventType === 'tool_execution_start') {
    recordToolStart(record, eventObject);
  } else if (eventType === 'tool_result' || eventType === 'tool_execution_end') {
    recordToolEnd(record, eventObject);
  } else if (eventType === 'response') {
    pushCapped(record.responses, event);
    const resp = event as { id?: string; success?: boolean; command?: string; error?: string };
    // A correlated reply to a liveness probe: resolve the pending probe so the
    // waiter learns the worker is alive-but-quiet (not hung). Any response at all
    // proves the RPC channel is live, so it also counts as a heartbeat below.
    if (resp.id && record.pendingProbes.has(resp.id)) {
      const resolveProbe = record.pendingProbes.get(resp.id)!;
      record.pendingProbes.delete(resp.id);
      resolveProbe();
    }
    if (resp.success === false) {
      if (!record.error) record.error = resp.error ?? `RPC command failed: ${resp.command ?? 'unknown'}`;
      pushLedgerEvent(record, 'error', record.error);
    }
    // Heartbeat on every response (success or not) so a blocking wait resets its
    // silence watchdog whenever the channel proves live.
    touch(record);
  } else if (eventType === 'agent_start') {
    // A structured result belongs to the turn that just ended. Clear it before
    // exposing the new turn as running, otherwise a prior [DONE]/[BLOCKED]
    // overrides the live process state in the footer and ledger.
    record.normalizedResult = undefined;
    record.deltaSummary = undefined;
    record.thinkingSummary = undefined;
    // ONE queued turn has started: decrement (never hard-reset) the pending
    // counter, so when two follow-ups are queued the ledger keeps showing
    // 'queued' work and agent_end after turn 1 does not resolve `wait` while
    // turn 2 has yet to run.
    record.pendingMessages = Math.max(0, (record.pendingMessages ?? 0) - 1);
    touch(record, 'running');
  } else if (eventType === 'message_end' && (event as { message?: unknown }).message) {
    const message = (event as { message: unknown }).message;
    pushCapped(record.messages, message);
    captureMessageError(record, message);
    updateLastOutput(record, message);
    recordInboundMessage(record, message);
    touch(record);
  } else if (eventType === 'agent_end') {
    const messages = (event as { messages?: unknown[] }).messages;
    if (Array.isArray(messages)) {
      for (const message of messages) {
        captureMessageError(record, message);
        updateLastOutput(record, message);
        recordInboundMessage(record, message);
      }
    }
    // agent_end {willRetry:true} means the worker aborted on context overflow
    // and Pi is compacting + retrying the turn — it is still working, so a
    // pending wait must not resolve with the incomplete lastOutput.
    // willRetry (context-overflow retry) or a still-pending queued turn both mean the
    // worker is not actually done — keep it non-terminal and do not resolve waiters.
    if ((event as { willRetry?: boolean }).willRetry === true || record.pendingMessages > 0) {
      touch(record);
    } else {
      touch(record, 'idle');
      notifyWaiters(record);
    }
  }
}

/**
 * Send an RPC message to the spawned agent process.
 * Returns true on success, false on failure (EPIPE / ERR_STREAM_WRITE_AFTER_END).
 * On failure the record is transitioned to 'failed' and all waiters are notified
 * so AgentMessage action:'wait' resolves immediately instead of hanging to timeout.
 */
function sendRpc(record: AgentRecord, payload: Record<string, unknown>, explicitId?: string): boolean {
  const id = explicitId ?? `${record.id}-${record.nextRequestId++}`;
  try {
    record.process.stdin.write(`${JSON.stringify({ id, ...payload })}\n`);
    return true;
  } catch (error) {
    // Writing to a destroyed/closed stdin throws EPIPE / ERR_STREAM_WRITE_AFTER_END.
    // H4: Transition to 'failed' and notify waiters — without this, any pending
    // action:'wait' would hang until timeout because the record stays in 'starting'.
    record.error = error instanceof Error ? error.message : String(error);
    touch(record, 'failed');
    notifyWaiters(record);
    return false;
  }
}

function workerAwarenessAgentId(workerId: string): string {
  const parentId = process.env[AWARENESS_AGENT_ENV_VAR]?.trim() || 'pi-agent';
  return `${parentId}:worker:${workerId.slice(0, 8)}`;
}

function cleanupPromptFiles(promptFiles: string[]): void {
  for (const filePath of promptFiles) {
    try { fs.rmSync(path.dirname(filePath), { recursive: true, force: true }); } catch { /* best-effort */ }
  }
}

function getActiveAgentUi(ctx?: PiContext): NonNullable<PiContext['ui']> | undefined {
  try {
    if (!ctx?.hasUI) return undefined;
    return ctx.ui;
  } catch {
    // Pi invalidates every ctx getter after session replacement/reload. Long-lived
    // worker callbacks may still drain afterward, so best-effort UI work must stop.
    return undefined;
  }
}

async function approveWorktreeIsolation(params: SpawnAgentParams, ctx?: PiContext): Promise<SpawnAgentParams> {
  if (params.isolation !== 'worktree') return params;
  const ui = getActiveAgentUi(ctx);
  if (typeof ui?.select !== 'function') {
    throw new Error('isolation:"worktree" requires an interactive UI approval; non-interactive hosts fail closed. Re-run with isolation:"shared" to use the current cwd intentionally.');
  }
  const create = 'Create isolated worktree';
  const shared = 'Use current repo / shared cwd';
  const cancel = 'Cancel spawn';
  const picked = await ui.select('Spawn this worker in an isolated git worktree?', [create, shared, cancel]);
  if (picked === create) return { ...params, worktreeDecision: 'create' };
  if (picked === shared) return { ...params, isolation: 'shared', worktreeDecision: 'shared' };
  throw new Error('Spawn cancelled before creating a worktree.');
}

function worktreeSnapshot(worktree: WorkerWorktreeState | undefined): WorkerWorktreeState | undefined {
  return worktree ? { ...worktree } : undefined;
}

function formatWorktreeState(worktree: WorkerWorktreeState | undefined): string {
  if (!worktree) return '';
  const branch = worktree.branch.replace(/^octocode\//, '');
  return ` ⎇ ${branch} +${worktree.aheadCommits}c ~${worktree.dirtyFiles}f ${worktree.mergeState}`;
}

function cleanupRecordWorktree(record: AgentRecord): void {
  if (!record.worktree || record.worktree.mergeState === 'discarded' || record.worktree.mergeState === 'merged') return;
  try {
    const outcome = cleanupWorktreeIfNoWork(record.worktree);
    pushLedgerEvent(record, 'worktree', outcome === 'removed' ? 'removed clean worktree' : 'kept unmerged worktree', record.worktree);
  } catch (cleanupError) {
    pushLedgerEvent(record, 'worktree', `worktree cleanup failed: ${cleanupError instanceof Error ? cleanupError.message : String(cleanupError)}`, record.worktree);
  }
}

export async function prepareSpawnAgentParams(params: SpawnAgentParams, ctx?: PiContext): Promise<SpawnAgentParams> {
  return approveWorktreeIsolation(params, ctx);
}

export function spawnRpcAgent(params: SpawnAgentParams, ctx?: PiContext): AgentRecord {
  if (!buildInitialPrompt(params)) throw new Error('spawnAgent requires task or prompt.');

  const id = randomUUID();
  const name = params.name ? String(params.name) : getRandomAgentName();
  const requestedCwd = path.resolve(String(params.cwd ?? ctx?.cwd ?? process.cwd()));
  const promptFiles: string[] = [];
  // SEV-1: workers resolve models against the same catalog as the parent, but Pi's
  // bare default (google/grok) is often unconfigured/unreachable — an unset worker
  // model silently errors every turn (0 tools run). Inherit the parent's known-working
  // model+provider when the caller didn't pin one, so delegation works by default.
  const effectiveParams = resolveWorkerModelParams({ ...params, cwd: requestedCwd }, ctx);
  validateWorkerModelParams(effectiveParams, ctx);
  const args = buildPiArgs(effectiveParams, name, promptFiles);
  const invocation = getPiInvocation(args);
  const awarenessAgentId = workerAwarenessAgentId(id);

  // M7: Enforce a hard cap on active (non-droppable) agents before spawning a new process.
  // Evict droppable (exited/failed/killed) agents first to reclaim slots, then refuse if
  // non-droppable agents still fill the registry. Checked before processFactory to ensure
  // no process is leaked when the cap is exceeded.
  evictStaleAgents();
  const policyResult = evaluateSpawnPolicy(effectiveParams, activeAgentCount());
  if (!policyResult.allowed) {
    cleanupPromptFiles(promptFiles);
    throw new Error(`${policyResult.reason} Kill or wait for existing agents before spawning more.`);
  }

  let worktree: InternalWorktreeState | undefined;
  let spawnParams = effectiveParams;
  let cwd = requestedCwd;
  if (effectiveParams.isolation === 'worktree') {
    if (effectiveParams.worktreeDecision !== 'create') {
      cleanupPromptFiles(promptFiles);
      throw new Error('isolation:"worktree" requires explicit user approval before creating a git worktree.');
    }
    worktree = createAgentWorktree({
      parentCwd: requestedCwd,
      agentId: id,
      name,
      includeUncommitted: effectiveParams.includeUncommitted,
    });
    spawnParams = withWorktreePromptContext(effectiveParams, worktree);
    cwd = worktree.path;
  }
  const peerIds = collectPeerAwarenessIds(id);
  const awarenessWorkspace = ctx?.cwd ?? requestedCwd;
  const parentAwarenessAgentId = process.env[AWARENESS_AGENT_ENV_VAR]?.trim() || 'pi-agent';
  const handback = prepareHandbackPath(awarenessWorkspace, id);
  const handbackPath = handback.path;
  const task = withPeerCoordination(buildInitialPrompt(spawnParams), awarenessAgentId, peerIds, {
    parentId: parentAwarenessAgentId,
    handbackPath,
  });

  let proc;
  try {
    proc = processFactory(invocation.command, invocation.args, {
      cwd,
      shell: false,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...process.env,
        [SUBAGENT_ENV_VAR]: '1',
        [AWARENESS_AGENT_ENV_VAR]: awarenessAgentId,
        // getPiInvocation() re-executes process.argv[1], which for any octocode-agent
        // process is bin/octocode-agent.mjs. Left unset, a worker spawned from a
        // parent running in the default SDK-embed mode would inherit that mode and
        // re-enter launchWithSdk() — whose arg parser does not understand
        // --tools/--exclude-tools/-e/--append-system-prompt/--skill, silently
        // dropping the curated allowlist buildPiArgs() just built. Force the
        // subprocess path, which forwards argv verbatim to the real Pi CLI.
        OCTOCODE_LAUNCHER_MODE: 'subprocess',
      },
    });
  } catch (error) {
    // processFactory threw before the record was added to `agents`, so removePromptFiles()
    // (wired to the record's 'close'/'error' handlers) would never run. Clean up the temp
    // system-prompt files buildPiArgs wrote so a failing factory does not leak files.
    cleanupPromptFiles(promptFiles);
    if (worktree) removeAgentWorktree(worktree, { force: true });
    throw error;
  }

  const record: AgentRecord = {
    id,
    name,
    cwd,
    command: invocation.command,
    args: invocation.args,
    task: String(effectiveParams.task ?? effectiveParams.prompt ?? '').trim(),
    planStep: effectiveParams.planStep?.trim() || undefined,
    process: proc,
    status: 'starting',
    startedAt: Date.now(),
    updatedAt: Date.now(),
    stderr: '',
    events: [],
    messages: [],
    responses: [],
    toolCalls: [],
    lastOutput: '',
    deltaSummary: undefined,
    handbackPath,
    thinkingSummary: undefined,
    pendingMessages: 0,
    normalizedResult: normalizeWorkerOutput(''),
    recoveryRisk: evaluateWorkerRecoveryRisk(''),
    ledgerEvents: [],
    policyWarnings: handback.warning
      ? [...policyResult.warnings, handback.warning]
      : policyResult.warnings,
    promptFiles,
    waiters: new Set(),
    activityListeners: new Set(),
    pendingProbes: new Map(),
    nextRequestId: 1,
    worktree,
    awarenessAgentId,
    awarenessWorkspace,
  };
  pushLedgerEvent(record, 'spawned', `spawned ${name}`, { awarenessAgentId });
  if (record.worktree) pushLedgerEvent(record, 'worktree', `created worktree ${record.worktree.branch}`, record.worktree);
  for (const warning of record.policyWarnings) pushLedgerEvent(record, 'policy', warning);
  agents.set(id, record);
  // Register the worker in the shared Awareness agent list (best-effort, advisory).
  syncWorkerRegistry('join', record);
  // Evict droppable agents to keep registry size ≤ MAX_AGENT_RECORDS.
  // The pre-spawn call (M7 cap check) runs before processFactory to avoid leaking
  // a process when the non-droppable cap is exceeded. This post-set call cleans up
  // droppable (exited/failed/killed) agents after the new record is in the map so
  // the total registry size stays bounded even when non-droppable count < cap.
  evictStaleAgents();

  let stdoutBuffer = '';
  // Decode incrementally: a multibyte UTF-8 sequence split across two `data`
  // chunks must not be turned into replacement chars — inside a JSON RPC line
  // that corruption makes JSON.parse throw and the event is silently dropped.
  const rpcDecoder = new StringDecoder('utf8');
  proc.stdout.on('data', (chunk) => {
    stdoutBuffer += rpcDecoder.write(chunk);
    const lines = stdoutBuffer.split('\n');
    stdoutBuffer = lines.pop() ?? '';
    for (const line of lines) processRpcLine(record, line);
    refreshAgentLedgerUi(ctx);
  });
  proc.stderr.on('data', (chunk) => {
    record.stderr += chunk.toString();
    pushLedgerEvent(record, 'status', 'stderr received');
    touch(record);
    refreshAgentLedgerUi(ctx);
  });
  proc.on('error', (error) => {
    record.error = error instanceof Error ? error.message : String(error);
    pushLedgerEvent(record, 'error', record.error);
    // Dead process: no agent_start will ever arrive to drain queued turns, so
    // strand pendingMessages at zero or the record never becomes terminal.
    record.pendingMessages = 0;
    touch(record, 'failed');
    removePromptFiles(record);
    cleanupRecordWorktree(record);
    syncWorkerRegistry('leave', record);
    notifyWaiters(record);
    refreshAgentLedgerUi(ctx);
  });
  proc.on('close', (code, signal) => {
    stdoutBuffer += rpcDecoder.end();
    if (stdoutBuffer.trim()) processRpcLine(record, stdoutBuffer);
    stdoutBuffer = '';
    record.exitCode = typeof code === 'number' ? code : undefined;
    record.signal = typeof signal === 'string' ? signal : undefined;
    // Process is gone: any queued turn that never reached agent_start is stranded,
    // so floor the counter or isTerminal() (and thus `wait`) never resolves and the
    // ledger keeps showing 'queued' against a dead worker.
    record.pendingMessages = 0;
    if (record.status !== 'killed') touch(record, code === 0 ? 'exited' : 'failed');
    pushLedgerEvent(record, record.status === 'failed' ? 'error' : 'exit', `process closed with code ${record.exitCode ?? 'unknown'}`);
    removePromptFiles(record);
    cleanupRecordWorktree(record);
    syncWorkerRegistry('leave', record);
    notifyWaiters(record);
    refreshAgentLedgerUi(ctx);
  });

  // H4: Only advance to 'running' when the initial RPC write succeeded.
  // If sendRpc returned false, it already transitioned the record to 'failed'
  // and notified waiters; overwriting with 'running' here would mask the failure.
  if (sendRpc(record, { type: 'prompt', message: task })) {
    pushLedgerEvent(record, 'message', 'initial prompt sent');
    touch(record, 'running');
  }
  // Make silent or slow-starting workers visible immediately. Event handlers will
  // keep the unified panel/footer fresh once stdout/stderr/close events arrive.
  refreshAgentLedgerUi(ctx);
  return record;
}

function summarizeAgent(record: AgentRecord, opts: { full?: boolean } = {}) {
  refreshNormalizedResult(record);
  const normalized = record.normalizedResult;
  const summaryText = normalized?.result || normalized?.next || record.lastOutput || record.stderr || record.error || '';
  const preview = truncateUserVisibleToolOutput(summaryText, 1000);
  return {
    agentId: record.id,
    name: record.name,
    status: record.status,
    cwd: record.cwd,
    model: getArgValue(record.args, '--model'),
    provider: getArgValue(record.args, '--provider'),
    task: record.task,
    planStep: record.planStep,
    thinking: getArgValue(record.args, '--thinking'),
    tools: getArgCsv(record.args, '--tools'),
    startedAt: new Date(record.startedAt).toISOString(),
    updatedAt: new Date(record.updatedAt).toISOString(),
    exitCode: record.exitCode,
    signal: record.signal,
    error: record.error,
    lastOutput: preview.text,
    outputTruncated: preview.truncated,
    normalizedResult: normalized,
    handback: statHandbackArtifact(record.handbackPath),
    recoveryRisk: record.recoveryRisk,
    pendingMessages: record.pendingMessages,
    lastMessage: record.lastMessage,
    thinkingSummary: record.thinkingSummary,
    policyWarnings: [...record.policyWarnings],
    ledgerEvents: opts.full ? [...record.ledgerEvents] : record.ledgerEvents.slice(-10),
    toolCalls: opts.full ? [...record.toolCalls] : record.toolCalls.slice(-10),
    activeTool: [...record.toolCalls].reverse().find((call) => call.status === 'running')?.toolName,
    worktree: worktreeSnapshot(record.worktree),
  };
}

function toWorkerLedgerEntry(record: AgentRecord): WorkerLedgerEntry {
  const normalized = record.normalizedResult;
  const activeTool = [...record.toolCalls].reverse().find((call) => call.status === 'running')?.toolName;
  const toolNames = [...new Set(record.toolCalls.map((call) => call.toolName).filter(Boolean))].slice(0, 4);
  return {
    agentId: record.id,
    name: record.name,
    status: record.status,
    startedAt: new Date(record.startedAt).toISOString(),
    updatedAt: new Date(record.updatedAt).toISOString(),
    model: getArgValue(record.args, '--model'),
    provider: getArgValue(record.args, '--provider'),
    task: record.task,
    planStep: record.planStep,
    thinking: getArgValue(record.args, '--thinking'),
    tools: getArgCsv(record.args, '--tools'),
    normalizedStatus: normalized?.status,
    result: normalized?.result,
    confidence: normalized?.confidence,
    evidence: normalized?.evidence,
    verification: normalized?.verification,
    next: normalized?.next,
    artifact: normalized?.artifact,
    deltaSummary: record.deltaSummary,
    handback: statHandbackArtifact(record.handbackPath),
    pendingMessages: record.pendingMessages,
    lastMessage: record.lastMessage,
    activeTool,
    toolCallCount: record.toolCalls.length,
    toolNames,
    worktree: worktreeSnapshot(record.worktree),
    recentEvents: record.ledgerEvents.slice(-10),
  };
}

export function listWorkerLedgerEntries(): WorkerLedgerEntry[] {
  return [...agents.values()]
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .map(toWorkerLedgerEntry);
}

function getArgValue(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function getArgCsv(args: string[], flag: string): string[] | undefined {
  const value = getArgValue(args, flag);
  return value ? value.split(',').map((item) => item.trim()).filter(Boolean) : undefined;
}

function formatAgentModelLine(summary: ReturnType<typeof summarizeAgent>): string {
  const model = summary.model ?? 'default model';
  const provider = summary.provider ? `${summary.provider}/` : '';
  const thinking = summary.thinking ? ` · think:${summary.thinking}` : '';
  const tools = summary.tools?.length ? ` · tools:${summary.tools.length}` : '';
  return `${provider}${model}${thinking}${tools}`;
}

function findAgentByIdOrPrefix(agentId: unknown): AgentRecord | undefined {
  const id = String(agentId ?? '').trim();
  if (!id) return undefined;
  return agents.get(id) ?? [...agents.values()].find((record) => record.id.startsWith(id));
}

function getAgent(agentId: unknown): AgentRecord {
  const id = String(agentId ?? '').trim();
  if (!id) throw new Error(
    'AgentMessage requires agentId for all actions except action:"list". '
    + 'Use action:"list" to see all active agents.',
  );
  const record = findAgentByIdOrPrefix(id);
  if (!record) throw new Error(
    `No agent found with id: ${id.slice(0, 16)}${id.length > 16 ? '\u2026' : ''}. `
    + `Use action:"list" to see all active agents (${agents.size} registered).`,
  );
  return record;
}

/** Why a `wait` returned. `terminal` = turn finished; `idle` = quiet gap the
 *  watchdog surfaced; `cap` = the optional absolute backstop fired. */
export type WaitReason = 'terminal' | 'idle' | 'cap';

export interface WaitOutcome {
  reason: WaitReason;
  /** True when the worker is not terminal — the turn is still in flight. */
  stillRunning: boolean;
  /** For reason:'idle' — whether a liveness probe got a reply (alive-but-quiet vs hung). */
  probedAlive: boolean;
}

export interface WaitOptions {
  /**
   * How long the worker may stream NO events before the watchdog checks in.
   * The timer resets on every inbound event, so an actively-working worker never
   * trips it — this bounds silence, not total runtime. Default 120s.
   */
  maxSilenceMs?: number;
  /**
   * Optional absolute backstop (total wall-clock) so a caller that must not block
   * forever can bound the wait even while the worker keeps proving itself alive.
   * Omit for the interactive path (silence + probe is enough).
   */
  absoluteCapMs?: number;
  /** Send a get_state liveness probe on a silence gap before declaring idle. Default true. */
  probe?: boolean;
  /** How long to await the probe's correlated response before giving up on it. Default 4s. */
  probeGraceMs?: number;
}

const DEFAULT_WAIT_MAX_SILENCE_MS = 120_000;
const DEFAULT_PROBE_GRACE_MS = 4_000;

/**
 * Actively confirm a quiet worker is alive by sending a `get_state` RPC and
 * awaiting its correlated `response` (pi RPC echoes the request id). Resolves
 * true if the child answers within graceMs, false if it stays silent or the
 * pipe is dead — distinguishing "alive but mid-generation" from "hung/crashed".
 */
function probeWorkerAlive(record: AgentRecord, graceMs: number): Promise<boolean> {
  if (!isProcessAlive(record)) return Promise.resolve(false);
  return new Promise((resolve) => {
    const probeId = `${record.id}-probe-${record.nextRequestId++}`;
    let settled = false;
    const finish = (alive: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      record.pendingProbes.delete(probeId);
      resolve(alive);
    };
    record.pendingProbes.set(probeId, () => finish(true));
    const timer = setTimeout(() => finish(false), graceMs);
    timer.unref?.();
    // get_state is a cheap read-only query; the child answers with a `response`
    // carrying probeId, which processRpcLine routes back to finish(true).
    if (!sendRpc(record, { type: 'get_state' }, probeId)) finish(false);
  });
}

/**
 * Progress-aware wait. Instead of a rigid wall-clock deadline that errors while
 * the worker is healthily churning, this resolves (never rejects) when the turn
 * ends, OR when the worker has been silent past maxSilenceMs AND a liveness probe
 * can't confirm it is still alive-and-quiet. Every inbound event resets the
 * silence watchdog, so long-but-active turns run to completion.
 */
export function waitForAgent(record: AgentRecord, options: WaitOptions = {}): Promise<WaitOutcome> {
  const maxSilenceMs = options.maxSilenceMs ?? DEFAULT_WAIT_MAX_SILENCE_MS;
  const probeGraceMs = options.probeGraceMs ?? DEFAULT_PROBE_GRACE_MS;
  const probeEnabled = options.probe ?? true;

  if (isTerminal(record)) {
    return Promise.resolve({ reason: 'terminal', stillRunning: false, probedAlive: false });
  }

  return new Promise((resolve) => {
    let settled = false;
    let silenceTimer: ReturnType<typeof setTimeout>;
    let absoluteTimer: ReturnType<typeof setTimeout> | undefined;

    const cleanup = () => {
      clearTimeout(silenceTimer);
      if (absoluteTimer) clearTimeout(absoluteTimer);
      record.waiters.delete(onTerminal);
      record.activityListeners.delete(onActivity);
    };
    const settle = (outcome: WaitOutcome) => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(outcome);
    };

    const armSilence = () => {
      clearTimeout(silenceTimer);
      silenceTimer = setTimeout(onSilence, maxSilenceMs);
      silenceTimer.unref?.();
    };

    const onTerminal = () => settle({ reason: 'terminal', stillRunning: false, probedAlive: false });

    const onActivity = () => {
      if (settled) return;
      // Terminal transitions also touch(); if this event flipped us terminal,
      // resolve as done rather than re-arming.
      if (isTerminal(record)) onTerminal();
      else armSilence();
    };

    const onSilence = () => {
      if (settled) return;
      if (isTerminal(record)) { onTerminal(); return; }
      if (!probeEnabled) {
        settle({ reason: 'idle', stillRunning: true, probedAlive: false });
        return;
      }
      void probeWorkerAlive(record, probeGraceMs).then((alive) => {
        if (settled) return;
        if (isTerminal(record)) { onTerminal(); return; }
        // Alive-but-quiet or hung/dead: either way hand a truthful snapshot back
        // to the caller (no error). Callers that need the final result loop until
        // reason:'terminal'; probedAlive tells them whether it's worth waiting more.
        settle({ reason: 'idle', stillRunning: !isTerminal(record), probedAlive: alive });
      });
    };

    record.waiters.add(onTerminal);
    record.activityListeners.add(onActivity);
    armSilence();

    if (options.absoluteCapMs && options.absoluteCapMs > 0) {
      absoluteTimer = setTimeout(() => {
        settle({ reason: 'cap', stillRunning: !isTerminal(record), probedAlive: isProcessAlive(record) });
      }, options.absoluteCapMs);
      absoluteTimer.unref?.();
    }
  });
}

/**
 * Block until the worker's turn genuinely finishes, transparently riding out
 * quiet-but-alive gaps. Loops the progress-aware waitForAgent, continuing while
 * the worker is idle-but-probe-alive, and stops on terminal, on a confirmed hang
 * (probe failed / process gone), or when the absolute cap is hit. Used by the
 * internal single-shot callers that need the final output, not a live snapshot.
 */
export async function waitForAgentTurn(
  record: AgentRecord,
  opts: { maxSilenceMs?: number; absoluteCapMs?: number } = {},
): Promise<WaitOutcome> {
  const startedAt = record.updatedAt;
  const absoluteCapMs = opts.absoluteCapMs;
  let outcome: WaitOutcome;
  do {
    const remaining = absoluteCapMs ? Math.max(1, absoluteCapMs - (Date.now() - startedAt)) : undefined;
    outcome = await waitForAgent(record, { maxSilenceMs: opts.maxSilenceMs, absoluteCapMs: remaining });
    if (outcome.reason === 'terminal' || outcome.reason === 'cap') break;
    // reason:'idle' — keep waiting only if the worker is still alive-and-quiet.
    if (!outcome.probedAlive || !isProcessAlive(record)) break;
  } while (!isTerminal(record));
  return outcome;
}

function renderAgentResult(records: AgentRecord[], header: string): ToolCallResult {
  const summaries = records.map((record) => summarizeAgent(record));
  const lines: string[] = [`${header} (${records.length}):`];
  for (const s of summaries) {
    const exit = s.exitCode !== undefined ? ` (exit ${s.exitCode})` : '';
    const elapsed = formatElapsed(
      new Date(s.startedAt).getTime(),
      isTerminal(s) ? new Date(s.updatedAt).getTime() : undefined,
    );
    const state = getAgentDisplayState(s);
    const meta = agentDisplayMeta(state);
    const handback = s.normalizedResult?.status && s.normalizedResult.status !== 'unknown'
      ? ` \u00b7 ${s.normalizedResult.status}/${s.normalizedResult.confidence}`
      : '';
    const latestEvent = s.ledgerEvents.at(-1)?.message;
    const result = s.normalizedResult?.result ?? s.normalizedResult?.next ?? s.lastOutput ?? latestEvent;
    const preview = result ? ` — ${result.slice(0, 60).replace(/\n/g, ' ')}${s.outputTruncated ? '…' : ''}` : '';
    const toolInfo = typeof s.activeTool === 'string' ? ` \u00b7 active:${s.activeTool}` : '';
    const modelInfo = ` \u00b7 ${formatAgentModelLine(s)}`;
    lines.push(`  ${meta.icon} ${s.name} (${shortId(s.agentId)}) \u00b7 ${meta.label}${exit}${handback}${modelInfo} \u00b7 ${elapsed}${toolInfo}${preview}`);
  }
  return {
    content: [{ type: 'text', text: lines.join('\n') }],
    details: { agents: summaries } satisfies AgentDetails,
  };
}

function countAgentStates(records: AgentDisplaySource[]): Record<AgentDisplayState, number> {
  const counts: Record<AgentDisplayState, number> = {
    starting: 0,
    queued: 0,
    running: 0,
    idle: 0,
    done: 0,
    blocked: 0,
    failed: 0,
    killed: 0,
  };
  for (const record of records) counts[getAgentDisplayState(record)] += 1;
  return counts;
}

function formatAgentStateCounts(records: AgentDisplaySource[]): string {
  const counts = countAgentStates(records);
  const order: AgentDisplayState[] = ['starting', 'queued', 'running', 'idle', 'blocked', 'done', 'failed', 'killed'];
  const parts = order
    .filter((state) => counts[state] > 0)
    .map((state) => `${counts[state]} ${state}`);
  return [`${records.length} total`, ...parts].join(SEP);
}

export function formatAgentLedger(): string {
  const records = [...agents.values()].sort((a, b) => b.updatedAt - a.updatedAt);
  if (records.length === 0) return 'Octocode agents: none';
  return `Octocode agents: ${formatAgentStateCounts(records)}`;
}

function buildAgentLedgerLines(limit = 10, theme?: PiTheme, width?: number): string[] {
  const records = [...agents.values()].sort((a, b) => b.updatedAt - a.updatedAt);
  const title = cliToolTitle(theme, 'Octocode agents');
  if (records.length === 0) return [`${title}: none`];

  const counts = formatAgentStateCounts(records);
  const lines = [`${title}: ${paint(theme, 'dim', counts)}`];
  for (const record of records.slice(0, limit)) {
    const summary = summarizeAgent(record);
    const state = getAgentDisplayState(summary);
    const meta = agentDisplayMeta(state, theme);
    const handback = summary.normalizedResult?.status && summary.normalizedResult.status !== 'unknown'
      ? ` · ${summary.normalizedResult.status}/${summary.normalizedResult.confidence}`
      : '';
    // meta.label already prints "running"; don't repeat it. Show only the tool the
    // worker is currently in, so the row reads "… · running · bash" not "· running · running bash".
    const active = summary.activeTool ? ` · ${paint(theme, 'brand', summary.activeTool)}` : '';
    // Show what the worker is doing: total tool calls + the distinct tools it has used.
    const callCount = record.toolCalls.length;
    const toolNames = [...new Set(record.toolCalls.map((call) => call.toolName).filter(Boolean))].slice(0, 4);
    const toolsInfo = callCount > 0
      ? ` · ${callCount} call${callCount === 1 ? '' : 's'}${toolNames.length ? ` [${toolNames.join(',')}${new Set(record.toolCalls.map((c) => c.toolName)).size > toolNames.length ? ',…' : ''}]` : ''}`
      : '';
    const modelInfo = ` · ${formatAgentModelLine(summary)}`;
    const taskInfo = summary.task
      ? ` · ${paint(theme, 'muted', `task ${summary.task.replace(/\s+/g, ' ').slice(0, 64)}`)}`
      : '';
    const planInfo = summary.planStep
      ? ` · ${paint(theme, 'symbol', `plan ${summary.planStep.replace(/\s+/g, ' ').slice(0, 48)}`)}`
      : '';
    // Stable queued indicator: reveal turns queued behind a running worker, or a
    // multi-deep queue. A single queued turn on a non-running worker already shows
    // via the 'queued' state label, so it is not duplicated here.
    const pending = summary.pendingMessages ?? 0;
    const queuedInfo = pending > 0 && (state !== 'queued' || pending > 1)
      ? ` · ${paint(theme, 'link', `queued ${pending}`)}`
      : '';
    const worktreeInfo = formatWorktreeState(summary.worktree);
    const latestEvent = summary.ledgerEvents.at(-1)?.message;
    const result = summary.normalizedResult?.result ?? summary.normalizedResult?.next ?? summary.lastOutput ?? latestEvent;
    const live = !isTerminal(record) && record.deltaSummary ? record.deltaSummary : undefined;
    const previewText = live ?? result;
    const preview = previewText ? ` — ${previewText.replace(/\n/g, ' ').slice(0, 90)}${!live && summary.outputTruncated ? '…' : ''}` : '';
    const name = paint(theme, 'brand', summary.name);
    const id = paint(theme, 'dim', shortId(summary.agentId));
    const elapsed = formatElapsed(record.startedAt, isTerminal(record) ? record.updatedAt : undefined);
    lines.push(`${meta.icon} ${name} (${id}) · ${meta.label}${handback}${queuedInfo}${modelInfo}${taskInfo}${planInfo}${active}${toolsInfo}${worktreeInfo} · ${elapsed}${paint(theme, 'dim', preview)}`);
    // Phase 3: a dim reasoning sub-line for live workers, gated to a small ledger so
    // it never crowds the panel. Distinct from the output preview (deltaSummary).
    if (record.thinkingSummary && !isTerminal(record) && records.length <= 3) {
      lines.push(paint(theme, 'dim', `    thinking: ${record.thinkingSummary.replace(/\n/g, ' ').slice(0, 80)}`));
    }
  }
  if (records.length > limit) lines.push(paint(theme, 'muted', `… ${records.length - limit} more; use AgentMessage list for full details.`));
  // Clip at the source when a width is known — pi errors on over-wide lines.
  return width ? lines.map((l) => truncateToWidth(l, width)) : lines;
}

export function formatAgentLedgerDetails(limit = 10): string {
  return buildAgentLedgerLines(limit).join('\n');
}

function hasVisibleAgentLedgerRecords(): boolean {
  return agents.size > 0 && !ledgerHidden;
}

/** The Agents section lines for the unified below-editor panel. Empty when there are no workers. */
export function agentPanelLines(theme?: PiTheme, limit = 6, width?: number): string[] {
  return hasVisibleAgentLedgerRecords() ? buildAgentLedgerLines(limit, theme, width) : [];
}

setStatusPanelAgentSource((theme, width) => agentPanelLines(theme, Number.MAX_SAFE_INTEGER, width));

/** Register the host-level footer/metrics refresher used by refreshAgentLedgerUi. */
export function setAgentLedgerMetricsRefreshForUi(cb: ((ctx?: PiContext) => void) | undefined): void {
  agentLedgerMetricsRefresh = cb;
}

function refreshAgentFooterMetrics(ctx?: PiContext): void {
  try {
    agentLedgerMetricsRefresh?.(ctx);
  } catch {
    // Footer refresh is best-effort UI work; ledger state must remain authoritative.
  }
}

/**
 * The single orchestrator for agent visibility. The custom footer is the sole
 * live ledger surface; event sources (ledger events, ticker, commands,
 * spawn/exit) call ONLY this so every state change repaints it exactly once.
 */
export function refreshAgentLedgerUi(ctx?: PiContext): void {
  const records = [...agents.values()];
  if (records.length === 0 || ledgerHidden) {
    stopLedgerTicker();
    refreshAgentFooterMetrics(ctx);
    return;
  }
  if (!getActiveAgentUi(ctx)) return;
  refreshAgentFooterMetrics(ctx);
  // Live refresh: while any worker is active, advance the spinner and re-render
  // every second on the shared ui-ticker clock (one timer process-wide).
  const anyActive = records.some((r) => !isTerminal(r));
  if (anyActive && !hasUiTickSubscriber(LEDGER_TICK_KEY)) {
    setUiTickSubscriber(LEDGER_TICK_KEY, () => {
      ledgerSpinnerFrame = (ledgerSpinnerFrame + 1) % LEDGER_SPINNER.length;
      if ([...agents.values()].some((r) => !isTerminal(r))) {
        refreshAgentLedgerUi(ctx);
      } else {
        stopLedgerTicker();
      }
    });
  } else if (!anyActive) {
    stopLedgerTicker();
  }
}

function formatOctocodeAgentsHelp(): string {
  return [
    OCTOCODE_AGENTS_COMMAND_USAGE,
    '',
    'Commands:',
    '- help — show this command reference',
    '- list/status — show the ledger and refresh footer/widget state',
    '- inspect <id-or-prefix> [full] — show worker state, handback, evidence, recent events, and stderr; "full" returns the complete tool-call/ledger/evidence history instead of the truncated preview',
    '- kill <id-or-prefix> — stop one live worker',
    '- kill-all — stop every live worker',
    '- prune — remove completed idle records from the in-memory ledger',
    '- hide — clear the footer/widget ledger for this session',
    '',
    'Spawning/use:',
    '- typed specialists: spawnSubagent({agent:"researcher"|"planner"|"architect", task:"..."}) · browser work: browserAgent tool',
    '- generic worker: spawnAgent({task:"...", name:"..."})',
    '- after spawning: AgentMessage({action:"wait"|"status"|"send"|"kill", agentId:"..."})',
    '- visible UI: running/blocked/failed/done workers appear in the unified status panel and compact footer until hide/prune/remove',
    '',
    'Tip: ids can be full ids or short prefixes shown by list/status.',
  ].join('\n');
}

function showAgentInspectionPanel(ctx?: PiContext): void {
  if (!ctx?.hasUI) return;
  resumeStatusPanel();
  refreshStatusPanel(ctx);
}

export async function handleOctocodeAgentsCommand(args: string, ctx?: PiContext): Promise<void> {
  const parts = args.trim().split(/\s+/).filter(Boolean);
  const action = (parts[0] || 'list').toLowerCase();
  const targetRaw = parts[1];
  const full = parts.slice(2).some((flag) => /^(?:-v|--verbose|--full|full)$/i.test(flag));
  if (action === 'help' || action === '--help' || action === '-h' || action === '?') {
    ctx?.ui?.notify?.(formatOctocodeAgentsHelp(), 'info');
    return;
  }
  if (action === 'hide' || action === 'clear') {
    ledgerHidden = true;
    refreshAgentFooterMetrics(ctx);
    ctx?.ui?.notify?.('Octocode agent ledger hidden for this session. Run /octocode-agents list to show it again.', 'info');
    return;
  }
  if (action === 'prune') {
    const droppable = [...agents.entries()].filter(([, record]) => isDroppable(record));
    for (const [id, record] of droppable) {
      removePromptFiles(record);
      agents.delete(id);
    }
    refreshAgentLedgerUi(ctx);
    ctx?.ui?.notify?.(`Pruned ${droppable.length} Octocode agent record(s).\n${formatAgentLedgerDetails()}`, 'info');
    return;
  }
  if (action === 'inspect') {
    const record = findAgentByIdOrPrefix(targetRaw);
    if (!record) {
      ctx?.ui?.notify?.(`No Octocode agent matches: ${targetRaw ?? '(missing id)'}\n${formatAgentLedgerDetails()}`, 'error');
      return;
    }
    refreshAgentLedgerUi(ctx);
    const fullText = (renderSingleAgentResult(record, 'Agent status', { full }).content[0] as { text?: string } | undefined)?.text ?? '';
    const preview = truncateUserVisibleToolOutput(fullText, MAX_AGENT_VIEW_CHARS);
    ctx?.ui?.notify?.(`${preview.text}${preview.truncated ? `\n… ${preview.omittedChars} chars hidden in this UI view` : ''}`, 'info');
    return;
  }
  if (action === 'kill-all') {
    const alive = [...agents.values()].filter((record) => !isDroppable(record));
    for (const record of alive) killAgent(record);
    refreshAgentLedgerUi(ctx);
    ctx?.ui?.notify?.(`Killed ${alive.length} Octocode agent(s).\n${formatAgentLedgerDetails()}`, 'warning');
    return;
  }
  if (action === 'kill') {
    const record = findAgentByIdOrPrefix(targetRaw);
    if (!record) {
      ctx?.ui?.notify?.(`No Octocode agent matches: ${targetRaw ?? '(missing id)'}\n${formatAgentLedgerDetails()}`, 'error');
      return;
    }
    killAgent(record);
    refreshAgentLedgerUi(ctx);
    ctx?.ui?.notify?.(`Killed Octocode agent ${record.name} (${shortId(record.id)}).\n${formatAgentLedgerDetails()}`, 'warning');
    return;
  }
  if (action !== 'list' && action !== 'status') {
    ctx?.ui?.notify?.(formatOctocodeAgentsHelp(), 'warning');
    return;
  }
  ledgerHidden = false;
  refreshAgentLedgerUi(ctx);
  showAgentInspectionPanel(ctx);
  ctx?.ui?.notify?.(formatAgentLedgerDetails(), 'info');
}

function renderSingleAgentResult(record: AgentRecord, header: string, opts: { full?: boolean } = {}): ToolCallResult {
  const output = record.lastOutput || record.stderr || record.error || '';
  const summary = summarizeAgent(record, opts);
  const elapsed = formatElapsed(record.startedAt, isTerminal(record) ? record.updatedAt : undefined);
  const statusParts = [
    `status: ${record.status}`,
    record.exitCode !== undefined ? `exit: ${record.exitCode}` : '',
    `elapsed: ${elapsed}`,
    record.error ? `error: ${record.error}` : '',
  ].filter(Boolean).join(' \u00b7 ');
  const contentParts: string[] = [
    `${header} [${record.name}]`,
    `agentId: ${record.id}`,
    statusParts,
  ];
  const toolSummary = formatToolCalls(record.toolCalls, opts.full ? record.toolCalls.length : 3);
  if (toolSummary) contentParts.push(`tools: ${toolSummary}`);
  if (summary.policyWarnings?.length) contentParts.push(`policy: ${summary.policyWarnings.join(' | ')}`);
  if (summary.normalizedResult?.status && summary.normalizedResult.status !== 'unknown') {
    contentParts.push(`handback: ${summary.normalizedResult.status} · confidence: ${summary.normalizedResult.confidence}`);
    if (summary.normalizedResult.result) contentParts.push(`result: ${summary.normalizedResult.result}`);
    if (summary.normalizedResult.evidence.length > 0) {
      const evidenceLimit = opts.full ? summary.normalizedResult.evidence.length : 3;
      contentParts.push(`evidence: ${summary.normalizedResult.evidence.slice(0, evidenceLimit).join('; ')}`);
    }
    if (summary.normalizedResult.verification) contentParts.push(`verification: ${summary.normalizedResult.verification}`);
    if (summary.normalizedResult.artifact) contentParts.push(`artifact: ${summary.normalizedResult.artifact}`);
    if (summary.normalizedResult.next) contentParts.push(`next: ${summary.normalizedResult.next}`);
  }
  const assignedHandback = summary.handback;
  contentParts.push(`handback file: ${assignedHandback.path}${assignedHandback.exists ? ` (${assignedHandback.bytes ?? 0} bytes)` : ' (not written yet)'}`);
  if (summary.recoveryRisk?.warnings.length) {
    contentParts.push(`recovery-risk: ${summary.recoveryRisk.warnings.join(' | ')}`);
  }
  if (summary.worktree) {
    contentParts.push(`worktree: ${summary.worktree.branch} @ ${summary.worktree.path} (+${summary.worktree.aheadCommits} commits, ~${summary.worktree.dirtyFiles} files, ${summary.worktree.mergeState})`);
  }
  if (output) contentParts.push('', output);
  return {
    content: [{ type: 'text', text: contentParts.join('\n') }],
    details: {
      agent: summary,
      output,
    },
    isError: record.status === 'failed' || Boolean(record.error),
  };
}

function killAgent(record: AgentRecord, opts: { forceKillDelayMs?: number } = {}): void {
  pushLedgerEvent(record, 'killed', 'kill requested');
  // The process is going away, so any queued turns will never emit agent_start
  // to decrement this. Strand them at zero here, or isTerminal() stays false
  // forever and a later `wait` blocks its full timeout on a dead worker while
  // the ledger advertises a phantom 'queued' state.
  record.pendingMessages = 0;
  touch(record, 'killed');
  try {
    record.process.stdin.end?.();
  } catch {
    // ignore stdin close errors
  }
  record.process.kill('SIGTERM');
  syncWorkerRegistry('leave', record);
  // NOTE: ChildProcess.killed only means "a signal was delivered", not "process
  // exited" — it is true immediately after SIGTERM above, so it cannot gate the
  // SIGKILL escalation. Gate on actual liveness (exitCode/signalCode still null).
  const forceKillDelayMs = opts.forceKillDelayMs ?? 5000;
  if (forceKillDelayMs <= 0) {
    if (isProcessAlive(record)) record.process.kill('SIGKILL');
  } else {
    setTimeout(() => {
      if (isProcessAlive(record)) record.process.kill('SIGKILL');
    }, forceKillDelayMs).unref?.();
  }
  removePromptFiles(record);
  notifyWaiters(record);
}

// ─── Programmatic worker seams ────────────────────────────────────────────────
// Thin exported wrappers over the exact code paths the AgentMessage tool and the
// /octocode-agents command verbs use, so other features can steer/kill/inspect
// workers without going through the tool surface.

/**
 * Steer a live worker by id or prefix. Running workers get the steer RPC
 * (redirects the in-flight turn, same as AgentMessage action:"steer"); idle
 * workers have no turn to redirect, so the message is queued via the follow_up
 * path (same as AgentMessage action:"followUp"). Returns false for unknown ids,
 * dead processes, or empty messages.
 */
export function steerWorkerById(idOrPrefix: string, message: string): boolean {
  const record = findAgentByIdOrPrefix(idOrPrefix);
  const text = String(message ?? '').trim();
  if (!record || !text || !isProcessAlive(record)) return false;
  if (record.status === 'running') {
    touch(record, 'running');
    const sent = sendRpc(record, { type: 'steer', message: text });
    if (sent) recordMessageActivity(record, 'to-agent', 'steer', text, `steer sent: ${previewMessage(text)}`);
    return sent;
  }
  const queued = sendRpc(record, { type: 'follow_up', message: text });
  if (queued) {
    enqueueWorkerTurn(record);
    recordMessageActivity(record, 'to-agent', 'follow-up', text, `follow-up queued: ${previewMessage(text)}`);
  }
  return queued;
}
/** Kill a worker by id or prefix (same path as /octocode-agents kill). Returns false for unknown ids. */
export function killWorkerById(idOrPrefix: string): boolean {
  const record = findAgentByIdOrPrefix(idOrPrefix);
  if (!record) return false;
  killAgent(record);
  return true;
}

/**
 * Render a worker's current state + output by id or prefix — the same
 * single-agent rendering AgentMessage status / /octocode-agents inspect use.
 * With maxLines set, keeps the LAST maxLines lines (the freshest output).
 * Returns undefined for unknown ids.
 */
export function getWorkerTranscript(idOrPrefix: string, opts: { maxLines?: number } = {}): string | undefined {
  const record = findAgentByIdOrPrefix(idOrPrefix);
  if (!record) return undefined;
  const text = (renderSingleAgentResult(record, 'Agent status').content[0] as { text?: string } | undefined)?.text ?? '';
  const maxLines = opts.maxLines;
  if (maxLines === undefined || maxLines <= 0) return text;
  const lines = text.split('\n');
  return lines.length <= maxLines ? text : lines.slice(-maxLines).join('\n');
}

export function registerAgentTools(
  pi: PiInstance,
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  if (process.env[SUBAGENT_ENV_VAR] === '1') return;
  installProcessCleanupHandlers();

  const resourceModeSchema = stringEnumSchema(
    Type,
    ['lean', 'octocode', 'default'],
    'Worker resource loading. lean disables extensions/skills/prompts/themes; octocode loads this extension explicitly; default uses Pi discovery.',
  );
  const actionSchema = stringEnumSchema(
    Type,
    ['list', 'status', 'send', 'steer', 'followUp', 'wait', 'kill', 'abort'],
    'AgentMessage action. abort sends Pi RPC abort (graceful interrupt without killing the process).',
  );

  registerFn(pi, registeredToolNames, {
    name: 'spawnAgent',
    label: 'Agent: Spawn Parallel Worker',
    description:
      'Spawn a separate background Pi worker process over RPC. Returns immediately with an agentId; use AgentMessage to inspect, send follow-ups, wait, or kill. Workers can run in parallel in the shared cwd by default, or in an explicitly approved git worktree with isolation:"worktree".',
    promptSnippet: 'Spawn a background Pi worker process and return an agentId for AgentMessage.',
    promptGuidelines: [
      'Use spawnAgent only when delegation materially helps: independent work ownership, long-running tasks, or adversarial/coverage checks.',
      'Do not spawn agents for ordinary bug fixes/refactors that need shared context; stay in the parent or batch independent tool calls instead.',
      'Before spawning, map dependencies and current Awareness ownership; delegate only one independent, bounded subtask per worker. Run two or more dependency-ready lanes in parallel only when their write ownership is disjoint; keep dependent or shared-file work serial.',
      'For useful parallelism, spawn all currently runnable independent workers first, then use AgentMessage action:"wait" or action:"status" to collect results.',
      'Ownership is exclusive: each packet must name exact owned paths or symbols plus read-only boundaries. The parent must not edit delegated paths until completion or explicit release; if overlap appears, stop one lane and coordinate a handoff or reassignment before resuming.',
      'Workers inherit no parent conversation. By default they share cwd/files/environment; pass isolation:"worktree" for an opt-in git worktree after explicit user approval, or isolation:"shared" when sharing is intentional.',
      'Structure the task as a labeled packet — lines starting with "Goal:", "Context:", "Scope:", "Ownership:", "Acceptance:", "Return:" (any of "-"/"—"/":" as separator, headings/bullets OK). A real gate checks for these labels, not just the words, and returns a [POLICY] warning on the spawn response when any are missing.',
      'spawnAgent defaults to resourceMode:"lean". Use resourceMode:"octocode" only when the worker needs Octocode extension tools.',
      'Model routing (which configured model to pass, `pi -ne --list-models`) is defined once in the agents policy — follow it there rather than re-deriving it here.',
      'Spawned-agent registry and output previews live in the current Pi process and are visible in /octocode-agents plus the custom footer ledger; collect needed results before session shutdown or reload.',
      'Pass planStep when the worker owns a parent-plan step; the footer and agent ledger show it with the stable task and effective model.',
      'Each worker packet includes an assigned durable handback file under .octocode/tmp/agents/<agentId>/handback.md; if the worker has write/bash capability, require important or long findings to be written there before terminal [DONE]/[BLOCKED]/[FAILED].',
      'spawnAgent prevents recursive subagents: workers never receive spawnAgent or AgentMessage, even in resourceMode:"octocode" or resourceMode:"default".',
    ],
    parameters: Type.Object({
      task: Type.Optional(Type.String({ description: 'Task for the worker. Required unless prompt is set.' })),
      prompt: Type.Optional(Type.String({ description: 'Alias for task.' })),
      context: Type.Optional(Type.String({ description: 'Self-contained context to prepend to the worker task.' })),
      name: Type.Optional(Type.String({ description: 'Human label for the worker/session.' })),
      cwd: Type.Optional(Type.String({ description: 'Working directory for the worker process. Defaults to current cwd.' })),
      model: Type.Optional(Type.String({ description: 'Pi model pattern or ID from `pi -ne --list-models [search]`. Choose from the live user-configured table; `--models` only sets model-cycling scope.' })),
      planStep: Type.Optional(Type.String({ description: 'Parent plan step this worker is executing; displayed in the agent ledger/footer.' })),
      provider: Type.Optional(Type.String({ description: 'Pi provider name for the model. REQUIRED when the model lives on a custom provider defined in models.json (e.g. "guy-provider-anthropic") — without it, pi resolves --model against builtin providers and may fail with "No API key found" or a 400. Look up via `pi -ne --list-models [search]`.' })),
      thinking: Type.Optional(Type.String({ description: 'Pi thinking level: off|minimal|low|medium|high|xhigh.' })),
      tools: Type.Optional(Type.Array(Type.String(), { description: 'Optional allowlist of enabled tool names for the worker. spawnAgent and AgentMessage are always removed.' })),
      systemPrompt: Type.Optional(Type.String({ description: 'Optional extra system prompt appended via a temporary file.' })),
      resourceMode: Type.Optional(resourceModeSchema),
      noSession: Type.Optional(Type.Boolean({ description: 'Pass --no-session to the worker. Default true.' })),
      isolation: Type.Optional(stringEnumSchema(Type, ['shared', 'worktree'], 'Worker filesystem isolation. "shared" (default) uses the current cwd; "worktree" asks before creating an isolated git worktree.')),
      includeUncommitted: Type.Optional(Type.Boolean({ description: 'With isolation:"worktree", apply a tracked-change snapshot from the parent tree using git stash create/apply. Untracked files are not included.' })),
    }),
    async execute(_toolCallId: string, params: Record<string, unknown>, _signal?: AbortSignal, _onUpdate?: unknown, ctx?: PiContext) {
      const approvedParams = await prepareSpawnAgentParams(params as SpawnAgentParams, ctx);
      const record = spawnRpcAgent(approvedParams, ctx);
      refreshAgentLedgerUi(ctx);
      return renderSingleAgentResult(record, 'Spawned agent');
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const p = args as Partial<SpawnAgentParams>;
      const name = String(p.name ?? 'worker');
      const task = String(p.task ?? p.prompt ?? '');
      const taskPreview = task.length > 72 ? `${task.slice(0, 72)}…` : (task || '(no task)');
      const model = p.model ? ` · ${p.model}` : '';
      const rawLine = [
        cliToolTitle(theme, 'spawnAgent', { bold: true }),
        paint(theme, 'brand', name),
        paint(theme, 'dim', `— ${taskPreview}${model}`),
      ].join(' ');
      return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return makeRenderer((w) => [truncateToWidth(paint(theme, 'brand', '⧗ Spawning agent…'), w)]);
      }
      const ok = !result.isError;
      const det = result.details as { agent?: { name?: string } } | null;
      const agentName = det?.agent?.name ?? 'agent';
      const displayStatus = ok ? 'spawned' : 'failed';
      // A just-spawned worker is RUNNING, not done — a green ✓ reads as "finished"
      // and disagrees with AgentMessage's brand in-flight glyph for the same worker.
      // Use the frozen (static, transcript-safe) brand running icon on success.
      const icon = ok ? agentDisplayMeta(getAgentDisplayState({ status: 'running' }), theme, { frozen: true }).icon : statusIcon('failed', theme);
      const label = cliToolTitle(theme, 'spawnAgent');
      const nameStr = paint(theme, 'brand', agentName);
      const statusStr = paint(theme, 'dim', displayStatus);
      const header = `${icon} ${label} · ${nameStr} · ${statusStr}`;
      if (!opts.expanded) {
        const hint = paint(theme, 'dim', ' · use AgentMessage wait/status');
        return makeRenderer((w) => [truncateToWidth(`${header}${hint}`, w)]);
      }
      return renderExpandedAgentResult(header, result, theme);
    },
  } satisfies ToolDefinition);
  registerFn(pi, registeredToolNames, {
    name: 'AgentMessage',
    label: 'Agent: Message Parallel Worker',
    description:
      'Manage spawned agents. Actions: list, status, send, steer, followUp, wait, kill, abort. Use this after spawnAgent to coordinate parallel workers.',
    promptSnippet: 'Message, wait for, list, status, or kill spawned background agents.',
    promptGuidelines: [
      'Use AgentMessage action:"list" or action:"status" before claiming a spawned worker is done; in the UI, also check /octocode-agents or the custom footer ledger for running/blocked/failed workers.',
      'Use AgentMessage action:"wait" to collect the current turn result. Idle means the turn ended, not necessarily that the delegated objective passed acceptance. A worker [DONE] means only its bounded objective passed; reconcile it and continue the parent plan while runnable authorized work remains.',
      'AgentMessage reads the in-memory spawned-agent registry; after session shutdown or reload, spawn fresh workers instead of relying on old agentIds.',
      'Before final answers, wait/status every relevant worker, reconcile disagreements, inspect any handback file shown by status/wait when it carries important or long findings, and synthesize findings instead of dumping raw worker JSON.',
      'Before AgentMessage kill/remove:true, use wait/status with full:true when the worker result matters; the assigned handback file is the durable fallback after registry removal.',
      'When you send, followUp, or steer work that changes scope, ownership, acceptance, or ordering, update the local plan in the same turn; if Awareness tasks/work are active, update those too so queued worker work is visible outside the message stream.',
      'Use action:"send" to start the next idle turn; while running it defaults to followUp. action:"followUp" queues after the turn. action:"steer" redirects after current tool calls, before the next model step.',
    ],
    parameters: Type.Object({
      action: Type.Optional(actionSchema),
      agentId: Type.Optional(Type.String({ description: 'Agent id from spawnAgent. Required except for action:"list".' })),
      message: Type.Optional(Type.String({ description: 'Message for send, steer, or followUp actions.' })),
      streamingBehavior: Type.Optional(
        stringEnumSchema(
          Type,
          ['steer', 'followUp'],
          'For action:"send", how to queue if the worker is currently streaming. Defaults to followUp only while the worker is already running.',
        ),
      ),
      timeoutMs: Type.Optional(Type.Integer({ description: 'For action:"wait": max silence (ms) tolerated before the wait checks in — NOT a hard deadline. An actively-streaming worker keeps the wait alive past this; it only returns early on a genuine quiet gap, and then with a live snapshot (never an error). Default 300000.' })),
      remove: Type.Optional(Type.Boolean({ description: 'After kill, remove the agent record from the registry.' })),
      full: Type.Optional(Type.Boolean({
        description:
          'For status/wait/kill/abort: return the complete tool-call and ledger history (up to the retained cap \u2014 ~200 tool calls, 80 ledger events, 8 evidence anchors) instead of the truncated preview (last 3 tool calls, last 10 ledger entries, first 3 evidence anchors). Use before trusting a worker\'s claim, not on every call \u2014 the preview is cheaper.',
      })),
    }),
    async execute(_toolCallId: string, params: Record<string, unknown>, _signal?: AbortSignal, _onUpdate?: unknown, ctx?: PiContext) {
      const action = (params['action'] as MessageAction | undefined) ?? 'status';
      const renderOpts = { full: params['full'] === true };
      if (action === 'list') {
        refreshAgentLedgerUi(ctx);
        return renderAgentResult([...agents.values()], 'Spawned agents');
      }

      const record = getAgent(params['agentId']);
      if (action === 'status') {
        refreshAgentLedgerUi(ctx);
        return renderSingleAgentResult(record, 'Agent status', renderOpts);
      }

      if (action === 'wait') {
        setManagedStatus(ctx, 'agent-wait', `\u29D7 Waiting for \u201C${record.name}\u201D\u2026`);
        // timeoutMs is the silence budget, not a rigid deadline: an actively
        // streaming worker keeps the wait alive indefinitely. On a genuine quiet
        // gap we probe liveness and return a truthful snapshot instead of erroring.
        let outcome: WaitOutcome;
        try {
          outcome = await waitForAgent(record, { maxSilenceMs: Number(params['timeoutMs'] ?? 300000) });
        } finally {
          setManagedStatus(ctx, 'agent-wait', undefined);
        }
        const header = outcome.reason === 'terminal'
          ? 'Agent turn completed'
          : outcome.probedAlive
            ? 'Agent still working (alive, no output during the wait window \u2014 call wait again to keep collecting)'
            : 'Agent unresponsive (no output and liveness probe unanswered \u2014 inspect with status or kill)';
        const waitResult = renderSingleAgentResult(record, header, renderOpts);
        if (params['remove'] === true) {
          // An idle (non-terminal) worker's process is still alive; deleting the
          // record would orphan it beyond the reach of shutdown cleanup.
          if (!isDroppable(record)) killAgent(record, { forceKillDelayMs: 0 });
          agents.delete(record.id);
        }
        refreshAgentLedgerUi(ctx);
        return waitResult;
      }

      if (action === 'kill') {
        killAgent(record);
        const result = renderSingleAgentResult(record, 'Agent killed', renderOpts);
        if (params['remove'] === true) agents.delete(record.id);
        refreshAgentLedgerUi(ctx);
        return result;
      }

      if (action === 'abort') {
        if (!isTerminal(record)) {
          // Graceful interrupt: the process stays alive and finishes aborting on its
          // own, then emits agent_end which resolves any pending wait via
          // notifyWaiters. We deliberately do NOT resolve waiters here — doing so
          // would report the turn as done while the worker is still unwinding.
          sendRpc(record, { type: 'abort' });
          touch(record);
        }
        refreshAgentLedgerUi(ctx);
        return renderSingleAgentResult(record, 'Agent aborted', renderOpts);
      }

      const message = String(params['message'] ?? '').trim();
      if (!message) throw new Error(`AgentMessage action:${action} requires message.`);
      // A dead worker's stdin is destroyed — writing to it throws EPIPE and would
      // wrongly flip the record back to 'running'. Reject with a clear error instead.
      if (!isProcessAlive(record)) {
        throw new Error(
          `AgentMessage action:${action} cannot reach agent "${record.name}" — it has ${record.status} (process exited). Spawn a fresh worker.`,
        );
      }
      // sendRpc self-handles a destroyed pipe (EPIPE): it sets status 'failed' and
      // notifies waiters internally, and isProcessAlive above already rejected the
      // dead-process case, so the boolean return needs no extra handling here.
      const wasRunning = record.status === 'running';
      if (action === 'steer') {
        // steer redirects an in-flight turn; on an idle worker there is no turn to
        // redirect yet, so it enqueues a turn instead — track it as pending rather
        // than faking 'running'.
        if (wasRunning) {
          touch(record, 'running');
          if (sendRpc(record, { type: 'steer', message })) {
            recordMessageActivity(record, 'to-agent', 'steer', message, `steer sent: ${previewMessage(message)}`);
          }
        } else if (sendRpc(record, { type: 'follow_up', message })) {
          // Idle workers have no in-flight turn to redirect — a bare `steer`
          // RPC would be dropped by Pi. Route through follow_up like
          // steerWorkerById so the message actually starts the next turn.
          enqueueWorkerTurn(record);
          recordMessageActivity(record, 'to-agent', 'steer', message, `steer queued: ${previewMessage(message)}`);
        }
      } else if (action === 'followUp') {
        // follow_up produces a turn that has not started yet (runs after the current
        // turn, or next when idle). Track it as pending so `wait` blocks and the
        // ledger shows 'queued' until the worker actually emits agent_start.
        if (sendRpc(record, { type: 'follow_up', message })) {
          enqueueWorkerTurn(record);
          recordMessageActivity(record, 'to-agent', 'follow-up', message, `follow-up queued: ${previewMessage(message)}`);
        }
      } else {
        // Default to followUp when the worker already has an in-flight or queued turn,
        // so back-to-back sends serialize behind it rather than racing.
        const busy = wasRunning || record.pendingMessages > 0;
        const streamingBehavior = params['streamingBehavior'] ?? (busy ? 'followUp' : undefined);
        if (sendRpc(record, {
          type: 'prompt',
          message,
          streamingBehavior,
        })) {
          // Either a queued follow-up or a fresh prompt to an idle worker: in both
          // cases the turn has not started, so mark it pending and let agent_start
          // flip the record to 'running'.
          enqueueWorkerTurn(record);
          recordMessageActivity(
            record,
            'to-agent',
            streamingBehavior === 'followUp' ? 'follow-up' : 'send',
            message,
            `${streamingBehavior === 'followUp' ? 'message queued' : 'message sent'}: ${previewMessage(message)}`,
          );
        }
      }
      refreshAgentLedgerUi(ctx);
      return renderSingleAgentResult(record, 'Agent messaged', renderOpts);
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const p = args as { action?: string; agentId?: string; message?: string };
      const action = String(p.action ?? 'status');
      const rec = p.agentId ? agents.get(p.agentId) : undefined;
      const agentLabel = rec
        ? paint(theme, 'brand', rec.name)
        : paint(theme, 'dim', p.agentId ? shortId(p.agentId) : 'all');
      const msgPart = p.message
        ? paint(theme, 'dim', ` — ${p.message.slice(0, 48)}${p.message.length > 48 ? '…' : ''}`)
        : '';
      const rawLine = [
        cliToolTitle(theme, 'AgentMessage', { bold: true }),
        paint(theme, 'brand', action),
        agentLabel,
        msgPart,
      ].filter(Boolean).join(' ');
      return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return makeRenderer((w) => [truncateToWidth(paint(theme, 'brand', '⧗ Agent working…'), w)]);
      }
      const ok = !result.isError;
      const det = result.details as {
        agent?: { name?: string; status?: AgentStatus } | null;
        agents?: Array<{ name: string; agentId: string; status: string; exitCode?: number }>;
        output?: string;
      } | null;
      if (det?.agents) {
        const squareIcon = paint(theme, 'title', '▦');
        const summaryText = formatAgentStateCounts(det.agents);
        const summary = paint(theme, 'dim', summaryText);
        const header = `${squareIcon} ${cliToolTitle(theme, 'AgentMessage')} list · ${summary}`;
        if (!opts.expanded) {
          return makeRenderer((w) => [truncateToWidth(header, w)]);
        }
        const text = result.content.find((p) => p.type === 'text')?.text ?? '';
        return makeRenderer((w) => [truncateToWidth(header, w), ...text.split('\n').slice(1).map((l) => truncateToWidth(paint(theme, 'dim', l), w))]);
      }
      // single-agent actions
      const agentName = det?.agent?.name ?? 'agent';
      const state = getAgentDisplayState(ok ? (det?.agent ?? { status: 'idle' }) : { status: 'failed' });
      // Result renderer → transcript entry: freeze the running glyph so repaints
      // don't animate the persisted row (contract: entry renderers are pure).
      const meta = agentDisplayMeta(state, theme, { frozen: true });
      const label = cliToolTitle(theme, 'AgentMessage');
      const nameStr = paint(theme, 'brand', agentName);
      const header = `${meta.icon} ${label} · ${nameStr} · ${meta.label}`;
      if (!opts.expanded) {
        const preview = det?.output ? det.output.split('\n').find((line) => line.trim())?.trim() : '';
        const suffix = preview ? ` — ${preview}` : ' · no output yet';
        return makeRenderer((w) => [truncateToWidth(`${header}${paint(theme, 'dim', suffix)}`, w)]);
      }
      return renderExpandedAgentResult(header, result, theme);
    },
  } satisfies ToolDefinition);
}
