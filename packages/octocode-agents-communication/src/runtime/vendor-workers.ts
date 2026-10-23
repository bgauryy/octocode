/** The managed vendor workers (Codex, Pi, Claude): launch arguments, handshake, event protocol and delivery. */
import type { ChildProcess } from 'node:child_process';
import { join } from 'node:path';
import * as catalog from './catalog.js';
import type { HeartbeatInput } from './catalog-types.generated.js';
import type { DeliveryRuntime, DeliveryStore } from './delivery-types.js';
import { ValidationError } from './errors.js';
import { dumps, encode } from './json-text.js';
import { validate } from './native-io.js';
import { realpath } from './paths.js';
import type { Offered } from './rows.js';
import { claude } from './transport.js';
import { codexPeerItems, codexPeerTurn } from './transport-codex.js';
import { isObject, objectOrEmpty, type JsonObject } from './validation.js';
import { VERSION } from './version.js';
import type { Wire } from './wire.js';

const PROXY_INSTRUCTIONS = 'The host manages your identity, presence, live lease renewal and delivery. Use bound tools for the user task. Initiate messages, broadcasts or subscriptions only when the task authorizes them. Follow the supplied skill for leases, replies and completion; unfinished work stays pending. Call inbox only for requested recovery. When no actionable work remains, end the turn. The host waits for mail and starts the next turn; do not sleep or poll to remain active.';

export interface ProxyArgs {
  vendor?: string;
  model?: string;
  prompt?: string;
  durationMs?: number;
  name?: string;
  session?: string;
  database?: string;
  workspace: string;
  tools?: string;
  trace?: boolean;
}

/** How a worker reaches the communication runtime from inside a vendor process; the interface layer owns the entry script. */
export interface WorkerLauncher {
  /** The command and arguments that start the bound MCP server (`mcp ...` flags are appended to `args`). */
  mcp(args: readonly string[]): { command: string; args: string[] };
  /** The CLI binary the Pi extension runs for each tool call (its `binding.binary`). */
  binary: string;
  /** The bundled Pi extension (`dist/hosts/pi-extension.js`) the worker loads. */
  piExtension: string;
}

export interface ProxyRuntime extends Pick<DeliveryRuntime, 'output' | 'open'> {
  launcher: WorkerLauncher;
  /** Starts the parent-death guard for the vendor process group; the module `watchdog` when omitted. A seam for ordering tests. */
  watchdog?: (group: number | undefined) => ChildProcess | null;
}

/** Text for the values the vendor error texts embed. */
function show(value: unknown): string {
  return typeof value === 'string' ? value : dumps(value ?? null);
}

/** A vendor reply field that must be text; anything else counts as missing. */
const textOf = (value: unknown): string => (typeof value === 'string' ? value : '');

/** What `Loop` reads and drives of a vendor worker: identity, store, wire, and the vendor's handshake, event and delivery steps. */
export interface LoopWorker {
  readonly name: string;
  readonly identity: string;
  readonly prompt: string;
  readonly toolNames: readonly string[];
  readonly deliverWhileBusy: boolean;
  readonly args: Pick<ProxyArgs, 'tools' | 'trace'>;
  readonly store: Pick<DeliveryStore, 'workspace' | 'coordinationScope' | 'known' | 'peerContext' | 'stage' | 'finishDispatch' | 'recordUsage'>;
  readonly host: Pick<Wire, 'pid' | 'event' | 'send'>;
  session: string;
  busy: boolean;
  emit(record: object): void;
  heartbeat(fields?: HeartbeatInput): Promise<void>;
  start(): Promise<void>;
  prime(peers: string): Promise<string>;
  begin(guidance: string): Promise<void>;
  onEvent(event: JsonObject, params: JsonObject, message: JsonObject, item: JsonObject, turn: JsonObject): Promise<void>;
  offer(items: readonly Offered[], content: string): Promise<void>;
  stageActionOnly(): boolean | null;
}

/** What one vendor worker is launched with: the run arguments, its bound store and identity, model, task, private directory and runtime. */
export interface WorkerSetup {
  args: ProxyArgs;
  store: DeliveryStore;
  identity: string;
  model: string;
  prompt: string;
  directory: string;
  runtime: ProxyRuntime;
}

/** One vendor's launch arguments, handshake, event protocol and delivery. */
abstract class VendorWorker implements LoopWorker {
  abstract readonly name: string;
  readonly deliverWhileBusy: boolean = true;
  readonly tools: ReturnType<typeof catalog.selectedTools>;
  readonly toolNames: string[];
  readonly mcp: { command: string; args: string[] };
  readonly cwd: string;
  readonly environment: Record<string, string> = {};
  host!: Wire;
  session = '';
  busy = true;

  readonly args: ProxyArgs;
  readonly store: DeliveryStore;
  readonly identity: string;
  readonly model: string;
  readonly prompt: string;
  readonly directory: string;
  readonly runtime: ProxyRuntime;

  constructor(setup: WorkerSetup) {
    const { args, store, identity, directory, runtime } = setup;
    this.args = args;
    this.store = store;
    this.identity = identity;
    this.model = setup.model;
    this.prompt = setup.prompt;
    this.directory = directory;
    this.runtime = runtime;
    this.tools = catalog.selectedTools(args.tools);
    const flags = ['mcp', '--workspace', store.workspace, '--database', store.database, '--session', identity];
    if (args.tools) flags.push('--tools', args.tools);
    this.mcp = runtime.launcher.mcp(flags);
    this.toolNames = this.tools.map(tool => tool.name);
    this.cwd = realpath(directory, true);
  }

  emit(record: object): void {
    this.runtime.output(record);
  }

  emitTexts(message: JsonObject): void {
    for (const piece of Array.isArray(message.content) ? message.content : []) if (isObject(piece) && piece.type === 'text') this.emit({ type: 'text', text: piece.text });
  }

  async heartbeat(fields: HeartbeatInput = {}): Promise<void> {
    await this.store.call(this.identity, 'heartbeat', { renewLeases: true, ...fields });
  }

  abstract argv(): string[];

  /** Vendor handshake before the first turn; may bind the vendor session. */
  async start(): Promise<void> {}

  /** Returns the peer context still owed to the first turn. */
  async prime(peers: string): Promise<string> {
    return peers;
  }

  async begin(guidance: string): Promise<void> {
    await this.deliver(guidance);
  }

  abstract deliver(content: string, action?: boolean): Promise<void>;

  async offer(items: readonly Offered[], content: string): Promise<void> {
    await this.deliver(content, items.some(item => item.wake === 'action'));
  }

  stageActionOnly(): boolean | null {
    return null;
  }

  /** One vendor event; `params` and `message` are its `params` and `message` objects (empty when absent). */
  abstract onEvent(event: JsonObject, params: JsonObject, message: JsonObject): Promise<void>;

  completed(): void {
    this.busy = false;
    this.emit({ type: 'turn-completed', vendor: this.name });
  }
}

class CodexWorker extends VendorWorker {
  readonly name = 'codex';

  argv(): string[] {
    return ['app-server'];
  }

  override async start(): Promise<void> {
    const host = this.host;
    await host.request('initialize', { clientInfo: { name: 'octocode-agents-communication', version: VERSION } });
    await host.send({ method: 'initialized', params: {} });
    await this.heartbeat();
    const config = objectOrEmpty(objectOrEmpty(await host.request('config/read', { includeLayers: false })).config);
    const plugins = Object.fromEntries(Object.keys(objectOrEmpty(config.plugins)).map(key => [key, { enabled: false }]));
    const servers: JsonObject = Object.fromEntries(Object.keys(objectOrEmpty(config.mcp_servers)).map(key => [key, { enabled: false }]));
    servers.agents_communication = { ...this.mcp, enabled: true, enabled_tools: this.toolNames, default_tools_approval_mode: 'approve' };
    const skills = await this.disabledSkills();
    await this.heartbeat();
    const started = objectOrEmpty(await host.request('thread/start', this.thread(servers, plugins, skills)));
    this.session = textOf(objectOrEmpty(started.thread).id);
    if (!this.session) throw new ValidationError('No vendor session ID');
    await this.heartbeat({ vendorSession: this.session });
  }

  private async disabledSkills(): Promise<JsonObject[]> {
    const discovered = objectOrEmpty(await this.host.request('skills/list', { cwds: [this.cwd], forceReload: true }));
    if (!Array.isArray(discovered.data)) throw new ValidationError('Invalid Codex skills list');
    const skills: JsonObject[] = [];
    for (const entry of discovered.data) {
      const listed = objectOrEmpty(entry).skills;
      if (!Array.isArray(listed)) throw new ValidationError('Invalid Codex skills entry');
      for (const skill of listed) {
        const path = objectOrEmpty(skill).path;
        if (typeof path !== 'string') throw new ValidationError('Missing Codex skill path');
        skills.push({ path, enabled: false });
      }
    }
    return skills;
  }

  private thread(servers: JsonObject, plugins: JsonObject, skills: JsonObject[]): JsonObject {
    const features = { code_mode: { enabled: false }, shell_tool: false, apply_patch_freeform: false, multi_agent: false, memories: false, hooks: false, apps: false, skill_search: false };
    const config = { mcp_servers: servers, plugins, project_doc_max_bytes: 0, skills: { config: skills }, web_search: 'disabled', features };
    return { model: this.model, cwd: this.cwd, approvalPolicy: 'never', sandbox: 'read-only', ephemeral: true, baseInstructions: PROXY_INSTRUCTIONS, developerInstructions: '', config };
  }

  override async prime(peers: string): Promise<string> {
    if (peers) await this.host.request('thread/inject_items', codexPeerItems(this.session, peers));
    return '';
  }

  override async begin(guidance: string): Promise<void> {
    await this.host.request('turn/start', { threadId: this.session, input: [{ type: 'text', text: guidance }], effort: 'low' });
  }

  async deliver(content: string, action = true): Promise<void> {
    if (action) await this.host.request('turn/start', codexPeerTurn(this.session, content));
    else await this.host.request('thread/inject_items', codexPeerItems(this.session, content));
  }

  override stageActionOnly(): boolean | null {
    return this.busy ? false : null;
  }

  async onEvent(event: JsonObject, params: JsonObject): Promise<void> {
    const method = event.method;
    const item = objectOrEmpty(params.item);
    const turn = objectOrEmpty(params.turn);
    if (method === 'item/completed' && item.type === 'agentMessage') this.emit({ type: 'text', text: item.text });
    if (method === 'turn/completed') {
      if (turn.status === 'failed') throw new Error('Vendor turn failed: ' + show(turn.error));
      this.completed();
    }
    if (method === 'error' && params.willRetry !== true) throw new Error('Vendor error: ' + show(params));
  }
}

class PiWorker extends VendorWorker {
  readonly name = 'pi';
  override readonly deliverWhileBusy = false;
  private readonly extension: string;
  private error: unknown = null;

  constructor(setup: WorkerSetup) {
    super(setup);
    const { store, identity, runtime } = setup;
    this.extension = runtime.launcher.piExtension;
    const binding = { binary: runtime.launcher.binary, workspace: store.workspace, database: store.database, session: identity, tools: this.tools };
    this.environment.OCTOCODE_COMMUNICATION_BINDING = encode(binding).toString('utf8');
  }

  argv(): string[] {
    return ['--mode', 'rpc', '--model', this.model, '--thinking', 'off', '--system-prompt', PROXY_INSTRUCTIONS, '--no-session', '--no-extensions', '--no-skills', '--no-prompt-templates', '--no-context-files', '--no-builtin-tools', '--extension', this.extension];
  }

  override async start(): Promise<void> {
    this.session = textOf(objectOrEmpty(await this.host.piRequest('get_state', {})).sessionId);
    if (!this.session) throw new ValidationError('No Pi session ID');
    await this.heartbeat({ vendorSession: this.session });
  }

  async deliver(content: string): Promise<void> {
    await this.host.piRequest('prompt', { message: content });
  }

  async onEvent(event: JsonObject, _params: JsonObject, message: JsonObject): Promise<void> {
    const kind = event.type;
    if (kind === 'response' && event.success === false) throw new Error('Pi command failed: ' + show(event.error));
    if (kind === 'message_end' && message.role === 'assistant') {
      const reason = message.stopReason;
      const failed = reason === 'error' || reason === 'aborted';
      this.error = failed ? (message.errorMessage ?? null) : null;
      if (failed && this.error === null) this.error = reason;
      this.emitTexts(message);
    }
    if (kind === 'agent_settled') {
      if (this.error !== null) throw new Error('Pi turn failed: ' + show(this.error));
      this.completed();
    }
  }
}

class ClaudeWorker extends VendorWorker {
  readonly name = 'claude';
  private readonly socket: string;

  constructor(setup: WorkerSetup) {
    super(setup);
    this.socket = join(this.cwd, 'inbox.sock');
    if (typeof process.geteuid !== 'function') throw new ValidationError('Managed Claude peer delivery requires local Unix sockets; use a host-wired communication binding on this platform');
    validate('claude', this.socket);
  }

  argv(): string[] {
    const settings = encode({ disableAllHooks: true, autoMemoryEnabled: false }).toString('utf8');
    const servers = encode({ mcpServers: { agents_communication: this.mcp } }).toString('utf8');
    return ['-p', '--messaging-socket-path', this.socket, '--settings', settings, '--system-prompt', PROXY_INSTRUCTIONS, '--model', this.model, '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose', '--no-session-persistence', '--setting-sources', '', '--strict-mcp-config', '--mcp-config', servers, '--tools', '', '--allowedTools', 'mcp__agents_communication__*', '--permission-mode', 'dontAsk', '--disable-slash-commands'];
  }

  async deliver(content: string): Promise<void> {
    await this.host.send({ type: 'user', message: { role: 'user', content }, session_id: this.session, parent_tool_use_id: null });
  }

  override async offer(items: readonly Offered[], content: string): Promise<void> {
    await claude(this.socket, this.session, items[0].dispatchToken, content);
  }

  async onEvent(event: JsonObject, _params: JsonObject, message: JsonObject): Promise<void> {
    const kind = event.type;
    if (kind === 'system' && event.subtype === 'init') {
      this.session = textOf(event.session_id);
      if (!this.session) throw new ValidationError('No vendor session ID');
      await this.heartbeat({ vendorSession: this.session });
    }
    if (kind === 'assistant') this.emitTexts(message);
    if (kind === 'result') {
      if (event.is_error === true) throw new Error('Vendor turn failed: ' + show(event));
      this.completed();
    }
  }
}

type WorkerClass = new (setup: WorkerSetup) => VendorWorker;

export const WORKERS: Record<string, WorkerClass> = { codex: CodexWorker, pi: PiWorker, claude: ClaudeWorker };
