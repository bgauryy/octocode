import type { Json, JsonObject } from '../../runtime/validation.js';

/** How a host starts the communication runtime: script entries run under the current Node, anything else runs as is. */
export function runtimeCommand(executable: string, args: readonly string[]): { command: string; args: string[] } {
  return /\.[cm]?js$/.test(executable)
    ? { command: process.execPath, args: [executable, ...args] }
    : { command: executable, args: [...args] };
}

/** The values a host binds a session to; every field is the runtime's own CLI flag. */
export interface HostBinding {
  binary: string;
  workspace: string;
  database: string;
  session: string;
}

/** The CLI flags that select a binding's workspace, database and session; an unset database or session is left to the runtime's defaults. */
export function bindingArgs({ workspace, database, session }: { workspace: string; database?: string; session?: string }): string[] {
  return ['--workspace', workspace, ...(database ? ['--database', database] : []), ...(session ? ['--session', session] : [])];
}

const BINDING_ENV = 'OCTOCODE_COMMUNICATION_BINDING';

/** The JSON object the parent process passes to a Pi adapter in `OCTOCODE_COMMUNICATION_BINDING`; unset means an empty binding. */
export function readBinding<T extends object>(env: NodeJS.ProcessEnv = process.env): Partial<T> {
  const raw = env[BINDING_ENV];
  if (!raw) return {};
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new Error(`${BINDING_ENV} is not valid JSON`); }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error(`${BINDING_ENV} must be a JSON object`);
  return value as Partial<T>;
}

/** What an adapter reads from Pi's session manager; the ledger entries are JSON Pi owns. */
export interface PiSessionManager {
  getSessionId(): string;
  getSessionFile?(): string | undefined;
  getEntries?(): JsonObject[];
}

/** The Pi context handed to every event: the working directory, the session and whether the agent is idle. */
export interface PiContext {
  cwd: string;
  sessionManager: PiSessionManager;
  isIdle?(): boolean;
}

/** Token counts of one assistant message; a provider may omit any of them. */
export interface PiUsage {
  input?: unknown;
  output?: unknown;
  cacheRead?: unknown;
  cacheWrite?: unknown;
}

/** The payload of each Pi event the adapters subscribe to; events they only observe carry nothing they read. */
export interface PiEvents {
  input: { source?: string };
  tool_call: { toolName: string; input?: { path?: unknown } };
  message_end: { message?: { role?: string; timestamp?: number | string; usage?: PiUsage } };
  before_provider_request: { payload?: JsonObject };
  cache_warming_decision: object;
  session_start: object;
  before_agent_start: object;
  agent_start: object;
  agent_end: object;
  session_shutdown: object;
}

/** A tool Pi runs for the model: the adapter returns the CLI result as text plus its parsed details. */
export interface PiTool {
  name: string;
  label: string;
  description: string;
  parameters: Record<string, unknown>;
  execute(id: string, input: Record<string, unknown>, signal?: AbortSignal): Promise<{ content: { type: 'text'; text: string }[]; details: Json }>;
}

/** The JSON the adapter stores with a ledger message; an unset field is left out when Pi serializes it. */
export type PiDetails = { [key: string]: Json | undefined };

/** A message the adapter appends to the Pi session. */
export interface PiMessage {
  customType: string;
  content: string;
  display: boolean;
  details?: PiDetails;
}

/** When the model sees an appended message: `triggerTurn` wakes an idle agent; `deliverAs` queues it as steering or a follow-up. */
export interface PiSendOptions {
  triggerTurn: boolean;
  deliverAs: 'steer' | 'followUp';
}

/** What bound tools need from Pi: the provider-request hook is optional (a host without it keeps the catalog's schemas). */
export interface PiToolHost {
  registerTool(tool: PiTool): void;
  on?<Name extends keyof PiEvents>(name: Name, handler: (event: PiEvents[Name], context: PiContext) => unknown): void;
}

/** The Pi extension API the adapters use; Pi owns its precise types and these are the parts read here. */
export interface PiHost extends PiToolHost {
  on<Name extends keyof PiEvents>(name: Name, handler: (event: PiEvents[Name], context: PiContext) => unknown): void;
  sendMessage(message: PiMessage, options: PiSendOptions): void;
}
