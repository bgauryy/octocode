import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { getInstallSource } from '../assets.js';
import { truncateUserVisibleToolOutput } from '../utils.js';
import type { PiContext, PiInstance, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

type AgentStatus = 'starting' | 'running' | 'idle' | 'exited' | 'failed' | 'killed';
type ResourceMode = 'lean' | 'octocode' | 'default';
type MessageAction = 'list' | 'status' | 'send' | 'steer' | 'followUp' | 'wait' | 'kill';

type StreamHandler = (event: string, cb: (chunk: Buffer | string) => void) => void;
type ProcessHandler = (event: string, cb: (...args: unknown[]) => void) => void;

interface AgentProcess {
  stdin: { write(data: string): unknown; end?(): unknown };
  stdout: { on: StreamHandler };
  stderr: { on: StreamHandler };
  on: ProcessHandler;
  kill(signal?: NodeJS.Signals): boolean;
  killed?: boolean;
}

interface SpawnOptions {
  cwd?: string;
  shell?: boolean;
  stdio?: Array<'ignore' | 'pipe'>;
  env?: NodeJS.ProcessEnv;
}

type AgentProcessFactory = (command: string, args: string[], options: SpawnOptions) => AgentProcess;

interface SpawnAgentParams {
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
  noContextFiles?: boolean;
  noSession?: boolean;
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
  lastOutput: string;
  promptFiles: string[];
  waiters: Set<() => void>;
  nextRequestId: number;
}

interface AgentDetails {
  agents: Array<ReturnType<typeof summarizeAgent>>;
}

const MAX_STORED_EVENTS = 200;
const MAX_VISIBLE_OUTPUT = 12000;
const SUBAGENT_ENV_VAR = 'OCTOCODE_PI_SUBAGENT';
const FORBIDDEN_WORKER_TOOLS = new Set(['spawnAgent', 'AgentMessage']);
const agents = new Map<string, AgentRecord>();
let processFactory: AgentProcessFactory = (command, args, options) => spawn(command, args, options) as unknown as AgentProcess;

export function setAgentProcessFactoryForTests(factory: AgentProcessFactory | null): void {
  processFactory = factory ?? ((command, args, options) => spawn(command, args, options) as unknown as AgentProcess);
  agents.clear();
}


function stringEnumSchema(
  Type: TypeBoxBuilder,
  values: readonly string[],
  description: string,
): Record<string, unknown> {
  return Type.Unsafe({ type: 'string', enum: [...values], description });
}

// ─── TUI rendering helpers ────────────────────────────────────────────────────

const ANSI_ESC_RE = /\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g;

function visibleWidth(str: string): number {
  return str.replace(ANSI_ESC_RE, '').length;
}

function truncateToWidth(str: string, maxWidth: number, ellipsis = '\u2026'): string {
  if (maxWidth <= 0) return '';
  if (visibleWidth(str) <= maxWidth) return str;
  let visible = 0;
  let out = '';
  let inEsc = false;
  for (const ch of str) {
    if (inEsc) { out += ch; if (/[@-~]/.test(ch)) inEsc = false; continue; }
    if (ch === '\x1B') { inEsc = true; out += ch; continue; }
    if (visible + 1 + ellipsis.length > maxWidth) break;
    out += ch;
    visible++;
  }
  return out + ellipsis;
}

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
  args.push('--name', name);
  args.push('--exclude-tools', [...FORBIDDEN_WORKER_TOOLS].join(','));

  if (params.provider) args.push('--provider', params.provider);
  if (params.model) args.push('--model', params.model);
  if (params.thinking) args.push('--thinking', params.thinking);
  if (workerTools.length) args.push('--tools', workerTools.join(','));
  if (params.noContextFiles) args.push('--no-context-files');

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

function processRpcLine(record: AgentRecord, line: string): void {
  if (!line.trim()) return;
  let event: unknown;
  try {
    event = JSON.parse(line);
  } catch {
    return;
  }

  pushCapped(record.events, event);
  const eventType = (event as { type?: string }).type;
  if (eventType === 'response') {
    pushCapped(record.responses, event);
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
  record.process.stdin.write(`${JSON.stringify({ id, ...payload })}\n`);
}

function spawnRpcAgent(params: SpawnAgentParams, ctx?: PiContext): AgentRecord {
  const task = buildInitialPrompt(params);
  if (!task) throw new Error('spawnAgent requires task or prompt.');

  const id = randomUUID();
  const name = params.name ? String(params.name) : `agent-${id.slice(0, 8)}`;
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
    lastOutput: '',
    promptFiles,
    waiters: new Set(),
    nextRequestId: 1,
  };
  agents.set(id, record);

  let stdoutBuffer = '';
  proc.stdout.on('data', (chunk) => {
    stdoutBuffer += chunk.toString();
    const lines = stdoutBuffer.split('\n');
    stdoutBuffer = lines.pop() ?? '';
    for (const line of lines) processRpcLine(record, line);
  });
  proc.stderr.on('data', (chunk) => {
    record.stderr += chunk.toString();
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
  };
}

function getArgValue(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function getAgent(agentId: unknown): AgentRecord {
  const id = String(agentId ?? '');
  const record = agents.get(id);
  if (!record) throw new Error(`Unknown agentId: ${id || '(missing)'}`);
  return record;
}

function waitForAgent(record: AgentRecord, timeoutMs: number): Promise<void> {
  if (['idle', 'exited', 'failed', 'killed'].includes(record.status)) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const onDone = () => {
      clearTimeout(timer);
      resolve();
    };
    const timer = setTimeout(() => {
      record.waiters.delete(onDone);
      reject(new Error(`Timed out waiting for ${record.id} after ${timeoutMs}ms.`));
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
    lines.push(`  ${s.name} (${shortId(s.agentId)}) \u00b7 ${s.status}${exit} \u00b7 ${elapsed}${preview}`);
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
    statusParts,
  ];
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

function killAgent(record: AgentRecord): void {
  touch(record, 'killed');
  try {
    record.process.stdin.end?.();
  } catch {
    // ignore stdin close errors
  }
  record.process.kill('SIGTERM');
  setTimeout(() => {
    if (!record.process.killed) record.process.kill('SIGKILL');
  }, 5000).unref?.();
  notifyWaiters(record);
}

export function registerAgentTools(
  pi: PiInstance,
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  if (process.env[SUBAGENT_ENV_VAR] === '1') return;

  const resourceModeSchema = stringEnumSchema(
    Type,
    ['lean', 'octocode', 'default'],
    'Worker resource loading. lean disables extensions/skills/prompts/themes; octocode loads this extension explicitly; default uses Pi discovery.',
  );
  const actionSchema = stringEnumSchema(
    Type,
    ['list', 'status', 'send', 'steer', 'followUp', 'wait', 'kill'],
    'AgentMessage action.',
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
      'spawnAgent prevents recursive subagents: workers never receive spawnAgent or AgentMessage, even in resourceMode:"octocode" or resourceMode:"default".',
    ],
    parameters: Type.Object({
      task: Type.Optional(Type.String({ description: 'Task for the worker. Required unless prompt is set.' })),
      prompt: Type.Optional(Type.String({ description: 'Alias for task.' })),
      context: Type.Optional(Type.String({ description: 'Self-contained context to prepend to the worker task.' })),
      name: Type.Optional(Type.String({ description: 'Human label for the worker/session.' })),
      cwd: Type.Optional(Type.String({ description: 'Working directory for the worker process. Defaults to current cwd.' })),
      model: Type.Optional(Type.String({ description: 'Pi model pattern or ID, e.g. sonnet:high or openai/gpt-4o.' })),
      provider: Type.Optional(Type.String({ description: 'Optional Pi provider name.' })),
      thinking: Type.Optional(Type.String({ description: 'Pi thinking level: off|minimal|low|medium|high|xhigh.' })),
      tools: Type.Optional(Type.Array(Type.String(), { description: 'Optional allowlist of enabled tool names for the worker. spawnAgent and AgentMessage are always removed.' })),
      systemPrompt: Type.Optional(Type.String({ description: 'Optional extra system prompt appended via a temporary file.' })),
      resourceMode: Type.Optional(resourceModeSchema),
      noContextFiles: Type.Optional(Type.Boolean({ description: 'Pass --no-context-files to the worker.' })),
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
      return { render: (w: number) => [truncateToWidth(rawLine, w)], invalidate() { /* no-op */ } };
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return {
          render: (w: number) => [truncateToWidth(theme?.fg('warning', '\u29D7 Spawning agent\u2026') ?? '\u29D7 Spawning agent\u2026', w)],
          invalidate() { /* no-op */ },
        };
      }
      const ok = !result.isError;
      const det = result.details as { agent?: { name?: string; status?: AgentStatus } } | null;
      const agentName = det?.agent?.name ?? 'agent';
      const agentStatus = det?.agent?.status ?? (ok ? 'running' : 'failed');
      const icon = statusIcon(ok ? agentStatus : 'failed', theme);
      const label = theme?.fg('toolTitle', 'spawnAgent') ?? 'spawnAgent';
      const nameStr = theme?.fg('accent', agentName) ?? agentName;
      const statusStr = theme?.fg('dim', agentStatus) ?? agentStatus;
      const header = `${icon} ${label} \u00b7 ${nameStr} \u00b7 ${statusStr}`;
      if (!opts.expanded) {
        return {
          render: (w: number) => [truncateToWidth(`${header}${theme?.fg('dim', ' \u00b7 expand for output') ?? ' \u00b7 expand for output'}`, w)],
          invalidate() { /* no-op */ },
        };
      }
      const text = result.content.find((p) => p.type === 'text')?.text ?? '';
      const outputLines = text.split('\n').slice(2); // skip agent-header + status lines
      return {
        render: (w: number) => [
          truncateToWidth(header, w),
          ...outputLines.map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w)),
        ],
        invalidate() { /* no-op */ },
      };
    },
  } satisfies ToolDefinition);
  registerFn(pi, registeredToolNames, {
    name: 'AgentMessage',
    label: 'Agent: Message Parallel Worker',
    description:
      'Manage spawned agents. Actions: list, status, send, steer, followUp, wait, kill. Use this after spawnAgent to coordinate parallel workers.',
    promptSnippet: 'Message, wait for, list, status, or kill spawned background agents.',
    promptGuidelines: [
      'Use AgentMessage action:"list" or action:"status" before claiming a spawned worker is done.',
      'Use AgentMessage action:"wait" to collect a worker result; use action:"kill" for stale or incorrect workers.',
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
        return renderSingleAgentResult(record, 'Agent completed');
      }

      if (action === 'kill') {
        killAgent(record);
        const result = renderSingleAgentResult(record, 'Agent killed');
        if (params['remove'] === true) agents.delete(record.id);
        return result;
      }

      const message = String(params['message'] ?? '').trim();
      if (!message) throw new Error(`AgentMessage action:${action} requires message.`);
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
      return { render: (w: number) => [truncateToWidth(rawLine, w)], invalidate() { /* no-op */ } };
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return {
          render: (w: number) => [truncateToWidth(theme?.fg('warning', '\u29D7 Agent working\u2026') ?? '\u29D7 Agent working\u2026', w)],
          invalidate() { /* no-op */ },
        };
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
          return { render: (w: number) => [truncateToWidth(header, w)], invalidate() { /* no-op */ } };
        }
        const text = result.content.find((p) => p.type === 'text')?.text ?? '';
        return {
          render: (w: number) => [truncateToWidth(header, w), ...text.split('\n').slice(1).map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w))],
          invalidate() { /* no-op */ },
        };
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
        return {
          render: (w: number) => [truncateToWidth(`${header}${theme?.fg('dim', ' \u00b7 expand for output') ?? ' \u00b7 expand for output'}`, w)],
          invalidate() { /* no-op */ },
        };
      }
      const text = result.content.find((p) => p.type === 'text')?.text ?? '';
      const outputLines = text.split('\n').slice(2); // skip agent-header + status lines
      return {
        render: (w: number) => [
          truncateToWidth(header, w),
          ...outputLines.map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w)),
        ],
        invalidate() { /* no-op */ },
      };
    },
  } satisfies ToolDefinition);
}
