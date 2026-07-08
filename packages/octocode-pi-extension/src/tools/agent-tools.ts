import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { getInstallSource } from '../assets.js';
import { truncateUserVisibleToolOutput } from '../utils.js';
import type { PiContext, PiInstance, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { stringEnumSchema } from './schema-helpers.js';
import { getRandomAgentName } from '../agentNames.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

export type AgentStatus = 'starting' | 'running' | 'idle' | 'exited' | 'failed' | 'killed';
export type ResourceMode = 'lean' | 'octocode' | 'default';
type MessageAction = 'list' | 'status' | 'send' | 'steer' | 'followUp' | 'wait' | 'kill' | 'abort';

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

export interface SpawnAgentParams {
  task?: string;
  prompt?: string;
  context?: string;
  name?: string;
  cwd?: string;
  model?: string;
  provider?: string;
  thinking?: string;
  tools?: string[];
  systemPrompt?: string;
  resourceMode?: ResourceMode;
  noSession?: boolean;
  /** Absolute paths to skill directories to load via --skill (additive, works with --no-skills). */
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
  promptFiles: string[];
  waiters: Set<() => void>;
  nextRequestId: number;
}

interface AgentDetails {
  agents: Array<ReturnType<typeof summarizeAgent>>;
}

const MAX_STORED_EVENTS = 200;
const MAX_STDERR_CHARS = 64_000;
const MAX_VISIBLE_OUTPUT = 12000;
const MAX_AGENT_RECORDS = 50;
const SUBAGENT_ENV_VAR = 'OCTOCODE_PI_SUBAGENT';
const FORBIDDEN_WORKER_TOOLS = new Set(['spawnAgent', 'AgentMessage']);
const agents = new Map<string, AgentRecord>();
const EXIT_SIGNALS: NodeJS.Signals[] = ['SIGTERM', 'SIGHUP'];
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
  agents.clear();
}

/** wait() resolves at end-of-turn: idle counts as "done for now", plus true terminals. */
function isTerminal(record: AgentRecord): boolean {
  return ['idle', 'exited', 'failed', 'killed'].includes(record.status);
}

/**
 * Safe to drop from the registry WITHOUT killing: the child process is gone.
 * `idle` is NOT droppable — an idle worker's process is still alive to accept
 * send/steer/followUp, so evicting or shutdown-skipping it would orphan the child.
 */
function isDroppable(record: AgentRecord): boolean {
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

export function cleanupSpawnedAgentsForShutdown(): number {
  // Kill every worker whose process is still alive — including idle ones, whose
  // process stays up between turns and would otherwise survive as an orphan.
  const alive = [...agents.values()].filter((record) => !isDroppable(record));
  for (const record of alive) killAgent(record, { forceKillDelayMs: 0 });
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

function statusIcon(status: AgentStatus, theme?: PiTheme): string {
  if (status === 'exited') return theme?.fg('success', '\u2713') ?? '\u2713'; // ✓
  if (status === 'failed') return theme?.fg('error', '\u2717') ?? '\u2717';   // ✗
  if (status === 'killed') return theme?.fg('warning', '\u2717') ?? '\u2717'; // ✗
  if (status === 'running') return theme?.fg('warning', '\u29D7') ?? '\u29D7'; // ⧗
  if (status === 'idle') return theme?.fg('success', '\u25CE') ?? '\u25CE';   // ◎
  return theme?.fg('dim', '\u25CB') ?? '\u25CB'; // ○ starting
}

function shortId(id: string): string {
  return id.slice(0, 8);
}

function formatElapsed(startedAt: number): string {
  const ms = Date.now() - startedAt;
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
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-agent-'));
  const filePath = path.join(dir, `${safeName(name)}.md`);
  fs.writeFileSync(filePath, text, { encoding: 'utf8', mode: 0o600 });
  return filePath;
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
  if (params.thinking) args.push('--thinking', params.thinking);
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
}

function notifyWaiters(record: AgentRecord): void {
  for (const waiter of record.waiters) waiter();
  record.waiters.clear();
}

function pushCapped<T>(items: T[], item: T): void {
  items.push(item);
  if (items.length > MAX_STORED_EVENTS) items.splice(0, items.length - MAX_STORED_EVENTS);
}

function extractTextFromMessage(message: unknown): string {
  const content = (message as { content?: unknown })?.content;
  if (!Array.isArray(content)) return '';
  return content
    .map((part) => ((part as { type?: string; text?: string }).type === 'text' ? (part as { text?: string }).text ?? '' : ''))
    .filter(Boolean)
    .join('\n');
}

function updateLastOutput(record: AgentRecord, message: unknown): void {
  const text = extractTextFromMessage(message);
  if (text) record.lastOutput = text;
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
    const resp = event as { success?: boolean; command?: string; error?: string };
    if (resp.success === false) {
      if (!record.error) record.error = resp.error ?? `RPC command failed: ${resp.command ?? 'unknown'}`;
      touch(record);
    }
  } else if (eventType === 'agent_start') {
    touch(record, 'running');
  } else if (eventType === 'message_end' && (event as { message?: unknown }).message) {
    const message = (event as { message: unknown }).message;
    pushCapped(record.messages, message);
    updateLastOutput(record, message);
    touch(record);
  } else if (eventType === 'agent_end') {
    const messages = (event as { messages?: unknown[] }).messages;
    if (Array.isArray(messages)) {
      for (const message of messages) updateLastOutput(record, message);
    }
    touch(record, 'idle');
    notifyWaiters(record);
  }
}

function sendRpc(record: AgentRecord, payload: Record<string, unknown>): void {
  const id = `${record.id}-${record.nextRequestId++}`;
  try {
    record.process.stdin.write(`${JSON.stringify({ id, ...payload })}\n`);
  } catch (error) {
    // Writing to a destroyed/closed stdin (child already exited) throws EPIPE /
    // ERR_STREAM_WRITE_AFTER_END. Surface it as the record error instead of
    // letting an unhandled stream error crash the host process.
    record.error = error instanceof Error ? error.message : String(error);
    touch(record);
  }
}

export function spawnRpcAgent(params: SpawnAgentParams, ctx?: PiContext): AgentRecord {
  const task = buildInitialPrompt(params);
  if (!task) throw new Error('spawnAgent requires task or prompt.');

  const id = randomUUID();
  const name = params.name ? String(params.name) : getRandomAgentName();
  const cwd = path.resolve(String(params.cwd ?? ctx?.cwd ?? process.cwd()));
  const promptFiles: string[] = [];
  const args = buildPiArgs(params, name, promptFiles);
  const invocation = getPiInvocation(args);
  const proc = processFactory(invocation.command, invocation.args, {
    cwd,
    shell: false,
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, [SUBAGENT_ENV_VAR]: '1' },
  });

  const record: AgentRecord = {
    id,
    name,
    cwd,
    command: invocation.command,
    args: invocation.args,
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
    promptFiles,
    waiters: new Set(),
    nextRequestId: 1,
  };
  agents.set(id, record);
  evictStaleAgents();

  let stdoutBuffer = '';
  proc.stdout.on('data', (chunk) => {
    stdoutBuffer += chunk.toString();
    const lines = stdoutBuffer.split('\n');
    stdoutBuffer = lines.pop() ?? '';
    for (const line of lines) processRpcLine(record, line);
  });
  proc.stderr.on('data', (chunk) => {
    record.stderr += chunk.toString();
    // Cap to the tail so a chatty worker can't grow this string unbounded.
    if (record.stderr.length > MAX_STDERR_CHARS) {
      record.stderr = record.stderr.slice(-MAX_STDERR_CHARS);
    }
    touch(record);
  });
  proc.on('error', (error) => {
    record.error = error instanceof Error ? error.message : String(error);
    touch(record, 'failed');
    removePromptFiles(record);
    notifyWaiters(record);
  });
  proc.on('close', (code, signal) => {
    if (stdoutBuffer.trim()) processRpcLine(record, stdoutBuffer);
    record.exitCode = typeof code === 'number' ? code : undefined;
    record.signal = typeof signal === 'string' ? signal : undefined;
    if (record.status !== 'killed') touch(record, code === 0 ? 'exited' : 'failed');
    removePromptFiles(record);
    notifyWaiters(record);
  });

  sendRpc(record, { type: 'prompt', message: task });
  touch(record, 'running');
  return record;
}

function summarizeAgent(record: AgentRecord) {
  const preview = truncateUserVisibleToolOutput(record.lastOutput || record.stderr || record.error || '', 1000);
  return {
    agentId: record.id,
    name: record.name,
    status: record.status,
    cwd: record.cwd,
    model: getArgValue(record.args, '--model'),
    startedAt: new Date(record.startedAt).toISOString(),
    updatedAt: new Date(record.updatedAt).toISOString(),
    exitCode: record.exitCode,
    signal: record.signal,
    error: record.error,
    lastOutput: preview.text,
    outputTruncated: preview.truncated,
    toolCalls: record.toolCalls.slice(-10),
    activeTool: [...record.toolCalls].reverse().find((call) => call.status === 'running')?.toolName,
  };
}

function getArgValue(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function getAgent(agentId: unknown): AgentRecord {
  const id = String(agentId ?? '').trim();
  if (!id) throw new Error(
    'AgentMessage requires agentId for all actions except action:"list". '
    + 'Use action:"list" to see all active agents.',
  );
  const record = agents.get(id);
  if (!record) throw new Error(
    `No agent found with id: ${id.slice(0, 16)}${id.length > 16 ? '\u2026' : ''}. `
    + `Use action:"list" to see all active agents (${agents.size} registered).`,
  );
  return record;
}

function waitForAgent(record: AgentRecord, timeoutMs: number): Promise<void> {
  if (isTerminal(record)) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const onDone = () => {
      clearTimeout(timer);
      resolve();
    };
    const timer = setTimeout(() => {
      record.waiters.delete(onDone);
      reject(new Error(`Timed out waiting for agent "${record.name}" after ${timeoutMs}ms. Use AgentMessage action:"status" to inspect.`));
    }, timeoutMs);
    record.waiters.add(onDone);
  });
}

function renderAgentResult(records: AgentRecord[], header: string): ToolCallResult {
  const summaries = records.map(summarizeAgent);
  const lines: string[] = [`${header} (${records.length}):`];
  for (const s of summaries) {
    const exit = s.exitCode !== undefined ? ` (exit ${s.exitCode})` : '';
    const elapsed = formatElapsed(new Date(s.startedAt).getTime());
    const preview = s.lastOutput ? ` \u2014 ${s.lastOutput.slice(0, 60).replace(/\n/g, ' ')}${s.outputTruncated ? '\u2026' : ''}` : '';
    const toolInfo = typeof s.activeTool === 'string' ? ` \u00b7 tool: ${s.activeTool}` : '';
    lines.push(`  ${s.name} (${shortId(s.agentId)}) \u00b7 ${s.status}${exit} \u00b7 ${elapsed}${toolInfo}${preview}`);
  }
  return {
    content: [{ type: 'text', text: lines.join('\n') }],
    details: { agents: summaries } satisfies AgentDetails,
  };
}

function renderSingleAgentResult(record: AgentRecord, header: string): ToolCallResult {
  const output = truncateUserVisibleToolOutput(record.lastOutput || record.stderr || record.error || '', MAX_VISIBLE_OUTPUT);
  const summary = summarizeAgent(record);
  const elapsed = formatElapsed(record.startedAt);
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
  const toolSummary = formatToolCalls(record.toolCalls);
  if (toolSummary) contentParts.push(`tools: ${toolSummary}`);
  if (output.text) contentParts.push('', output.text);
  if (output.truncated) contentParts.push(`\u2026 output truncated (${output.omittedChars} chars hidden; full content in details)`);
  return {
    content: [{ type: 'text', text: contentParts.join('\n') }],
    details: {
      agent: summary,
      output: output.text,
      outputTruncated: output.truncated,
      omittedChars: output.omittedChars,
    },
    isError: record.status === 'failed',
  };
}

function killAgent(record: AgentRecord, opts: { forceKillDelayMs?: number } = {}): void {
  touch(record, 'killed');
  try {
    record.process.stdin.end?.();
  } catch {
    // ignore stdin close errors
  }
  record.process.kill('SIGTERM');
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
      'Spawn a background Pi worker process over RPC. Returns immediately with an agentId; use AgentMessage to inspect, send follow-ups, wait, or kill. Workers are isolated processes and can run in parallel.',
    promptSnippet: 'Spawn a background Pi worker process and return an agentId for AgentMessage.',
    promptGuidelines: [
      'Use spawnAgent only when delegation materially helps: independent work ownership, long-running tasks, or adversarial/coverage checks.',
      'Do not spawn agents for ordinary bug fixes/refactors that need shared context; stay in the parent or batch independent tool calls instead.',
      'For useful parallelism, spawn all independent workers first, then use AgentMessage action:"wait" or action:"status" to collect results.',
      'spawnAgent defaults to resourceMode:"lean". Use resourceMode:"octocode" only when the worker needs Octocode extension tools.',
      'Use `pi -ne --list-models [search]` as the source of truth for the user-configured model table; do not read hardcoded config paths.',
      'Pass model for each worker: fastest capable configured model for small tasks, balanced coding/reasoning model for medium tasks, strongest configured model for large/high-risk work.',
      'Spawned-agent registry and output previews live in the current Pi process; collect needed results before session shutdown or reload.',
      'spawnAgent prevents recursive subagents: workers never receive spawnAgent or AgentMessage, even in resourceMode:"octocode" or resourceMode:"default".',
    ],
    parameters: Type.Object({
      task: Type.Optional(Type.String({ description: 'Task for the worker. Required unless prompt is set.' })),
      prompt: Type.Optional(Type.String({ description: 'Alias for task.' })),
      context: Type.Optional(Type.String({ description: 'Self-contained context to prepend to the worker task.' })),
      name: Type.Optional(Type.String({ description: 'Human label for the worker/session.' })),
      cwd: Type.Optional(Type.String({ description: 'Working directory for the worker process. Defaults to current cwd.' })),
      model: Type.Optional(Type.String({ description: 'Pi model pattern or ID from `pi -ne --list-models [search]`. Choose from the live user-configured table; `--models` only sets model-cycling scope.' })),
      provider: Type.Optional(Type.String({ description: 'Optional Pi provider name.' })),
      thinking: Type.Optional(Type.String({ description: 'Pi thinking level: off|minimal|low|medium|high|xhigh.' })),
      tools: Type.Optional(Type.Array(Type.String(), { description: 'Optional allowlist of enabled tool names for the worker. spawnAgent and AgentMessage are always removed.' })),
      systemPrompt: Type.Optional(Type.String({ description: 'Optional extra system prompt appended via a temporary file.' })),
      resourceMode: Type.Optional(resourceModeSchema),
      noSession: Type.Optional(Type.Boolean({ description: 'Pass --no-session to the worker. Default true.' })),
    }),
    async execute(_toolCallId: string, params: Record<string, unknown>, _signal?: AbortSignal, _onUpdate?: unknown, ctx?: PiContext) {
      const record = spawnRpcAgent(params as SpawnAgentParams, ctx);
      return renderSingleAgentResult(record, 'Spawned agent');
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const p = args as Partial<SpawnAgentParams>;
      const name = String(p.name ?? 'worker');
      const task = String(p.task ?? p.prompt ?? '');
      const taskPreview = task.length > 72 ? `${task.slice(0, 72)}\u2026` : (task || '(no task)');
      const model = p.model ? ` \u00b7 ${p.model}` : '';
      const rawLine = [
        theme?.fg('toolTitle', theme.bold('spawnAgent')) ?? 'spawnAgent',
        theme?.fg('accent', name) ?? name,
        theme?.fg('dim', `\u2014 ${taskPreview}${model}`) ?? `\u2014 ${taskPreview}${model}`,
      ].join(' ');
      return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return makeRenderer((w) => [truncateToWidth(theme?.fg('warning', '\u29D7 Spawning agent\u2026') ?? '\u29D7 Spawning agent\u2026', w)]);
      }
      const ok = !result.isError;
      const det = result.details as { agent?: { name?: string } } | null;
      const agentName = det?.agent?.name ?? 'agent';
      const displayStatus = ok ? 'spawned' : 'failed';
      const icon = ok ? (theme?.fg('success', '\u2713') ?? '\u2713') : statusIcon('failed', theme);
      const label = theme?.fg('toolTitle', 'spawnAgent') ?? 'spawnAgent';
      const nameStr = theme?.fg('accent', agentName) ?? agentName;
      const statusStr = theme?.fg('dim', displayStatus) ?? displayStatus;
      const header = `${icon} ${label} \u00b7 ${nameStr} \u00b7 ${statusStr}`;
      if (!opts.expanded) {
        return makeRenderer((w) => [truncateToWidth(`${header}${theme?.fg('dim', ' \u00b7 expand for output') ?? ' \u00b7 expand for output'}`, w)]);
      }
      const text = result.content.find((p) => p.type === 'text')?.text ?? '';
      const outputLines = text.split('\n').slice(2); // skip agent-header + status lines
      return makeRenderer((w) => [
        truncateToWidth(header, w),
        ...outputLines.map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w)),
      ]);
    },
  } satisfies ToolDefinition);
  registerFn(pi, registeredToolNames, {
    name: 'AgentMessage',
    label: 'Agent: Message Parallel Worker',
    description:
      'Manage spawned agents. Actions: list, status, send, steer, followUp, wait, kill, abort. Use this after spawnAgent to coordinate parallel workers.',
    promptSnippet: 'Message, wait for, list, status, or kill spawned background agents.',
    promptGuidelines: [
      'Use AgentMessage action:"list" or action:"status" before claiming a spawned worker is done.',
      'Use AgentMessage action:"wait" to collect a worker result; use action:"kill" for stale or incorrect workers.',
      'AgentMessage reads the in-memory spawned-agent registry; after session shutdown or reload, spawn fresh workers instead of relying on old agentIds.',
      'Before final answers, wait/status every relevant worker, reconcile disagreements, and synthesize findings instead of dumping raw worker JSON.',
      'Use AgentMessage action:"send" for follow-up instructions; action:"steer" interrupts the next turn; action:"followUp" queues after completion.',
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
      timeoutMs: Type.Optional(Type.Integer({ description: 'wait timeout in milliseconds. Default 300000.' })),
      remove: Type.Optional(Type.Boolean({ description: 'After kill, remove the agent record from the registry.' })),
    }),
    async execute(_toolCallId: string, params: Record<string, unknown>, _signal?: AbortSignal, _onUpdate?: unknown, ctx?: PiContext) {
      const action = (params['action'] as MessageAction | undefined) ?? 'status';
      if (action === 'list') return renderAgentResult([...agents.values()], 'Spawned agents');

      const record = getAgent(params['agentId']);
      if (action === 'status') return renderSingleAgentResult(record, 'Agent status');

      if (action === 'wait') {
        if (ctx?.hasUI) ctx.ui?.setStatus?.('agent-wait', `\u29D7 Waiting for \u201C${record.name}\u201D\u2026`);
        try {
          await waitForAgent(record, Number(params['timeoutMs'] ?? 300000));
        } finally {
          if (ctx?.hasUI) ctx.ui?.setStatus?.('agent-wait', '');
        }
        const waitResult = renderSingleAgentResult(record, 'Agent completed');
        if (params['remove'] === true) agents.delete(record.id);
        return waitResult;
      }

      if (action === 'kill') {
        killAgent(record);
        const result = renderSingleAgentResult(record, 'Agent killed');
        if (params['remove'] === true) agents.delete(record.id);
        return result;
      }

      if (action === 'abort') {
        if (!isTerminal(record)) {
          sendRpc(record, { type: 'abort' });
          touch(record);
        }
        return renderSingleAgentResult(record, 'Agent aborted');
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
      const wasRunning = record.status === 'running';
      touch(record, 'running');
      if (action === 'steer') {
        sendRpc(record, { type: 'steer', message });
      } else if (action === 'followUp') {
        sendRpc(record, { type: 'follow_up', message });
      } else {
        sendRpc(record, {
          type: 'prompt',
          message,
          streamingBehavior: params['streamingBehavior'] ?? (wasRunning ? 'followUp' : undefined),
        });
      }
      return renderSingleAgentResult(record, 'Agent messaged');
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const p = args as { action?: string; agentId?: string; message?: string };
      const action = String(p.action ?? 'status');
      const rec = p.agentId ? agents.get(p.agentId) : undefined;
      const agentLabel = rec
        ? (theme?.fg('accent', rec.name) ?? rec.name)
        : (theme?.fg('dim', p.agentId ? shortId(p.agentId) : 'all') ?? (p.agentId ? shortId(p.agentId) : 'all'));
      const msgPart = p.message
        ? (theme?.fg('dim', ` \u2014 ${p.message.slice(0, 48)}${p.message.length > 48 ? '\u2026' : ''}`) ?? ` \u2014 ${p.message.slice(0, 48)}`)
        : '';
      const rawLine = [
        theme?.fg('toolTitle', theme.bold('AgentMessage')) ?? 'AgentMessage',
        theme?.fg('accent', action) ?? action,
        agentLabel,
        msgPart,
      ].filter(Boolean).join(' ');
      return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return makeRenderer((w) => [truncateToWidth(theme?.fg('warning', '\u29D7 Agent working\u2026') ?? '\u29D7 Agent working\u2026', w)]);
      }
      const ok = !result.isError;
      const det = result.details as {
        agent?: { name?: string; status?: AgentStatus } | null;
        agents?: Array<{ name: string; agentId: string; status: string; exitCode?: number }>;
      } | null;
      // list action \u2014 compact agent count summary
      if (det?.agents) {
        const count = det.agents.length;
        const running = det.agents.filter((a) => a.status === 'running').length;
        const exited = det.agents.filter((a) => a.status === 'exited').length;
        const failed = det.agents.filter((a) => a.status === 'failed').length;
        const squareIcon = theme?.fg('toolTitle', '\u25A6') ?? '\u25A6';
        const summary = theme?.fg('dim', `${count} agents \u00b7 ${running} running \u00b7 ${exited} done \u00b7 ${failed} failed`) ?? `${count} agents`;
        const header = `${squareIcon} ${theme?.fg('toolTitle', 'AgentMessage') ?? 'AgentMessage'} list \u00b7 ${summary}`;
        if (!opts.expanded) {
          return makeRenderer((w) => [truncateToWidth(header, w)]);
        }
        const text = result.content.find((p) => p.type === 'text')?.text ?? '';
        return makeRenderer((w) => [truncateToWidth(header, w), ...text.split('\n').slice(1).map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w))]);
      }
      // single-agent actions
      const agentName = det?.agent?.name ?? 'agent';
      const agentStatus = det?.agent?.status ?? (ok ? 'idle' : 'failed');
      const icon = statusIcon(ok ? agentStatus : 'failed', theme);
      const label = theme?.fg('toolTitle', 'AgentMessage') ?? 'AgentMessage';
      const nameStr = theme?.fg('accent', agentName) ?? agentName;
      const statusStr = theme?.fg('dim', agentStatus) ?? agentStatus;
      const header = `${icon} ${label} \u00b7 ${nameStr} \u00b7 ${statusStr}`;
      if (!opts.expanded) {
        return makeRenderer((w) => [truncateToWidth(`${header}${theme?.fg('dim', ' \u00b7 expand for output') ?? ' \u00b7 expand for output'}`, w)]);
      }
      const text = result.content.find((p) => p.type === 'text')?.text ?? '';
      const outputLines = text.split('\n').slice(2); // skip agent-header + status lines
      return makeRenderer((w) => [
        truncateToWidth(header, w),
        ...outputLines.map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w)),
      ]);
    },
  } satisfies ToolDefinition);
}
