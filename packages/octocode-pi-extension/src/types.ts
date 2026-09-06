/**
 * Pi runtime API type definitions.
 *
 * Pi's published types define host contracts; local interfaces describe
 * Octocode's tool results, runtime state, and UI projections.
 */

import type { PromptMode } from '@octocodeai/octocode-shared/protocols';

import type {
  BuildSystemPromptOptions as PiBuildSystemPromptOptions,
  ContextUsage as PiContextUsage,
  ReadonlyFooterDataProvider,
  ToolDefinition as PiToolDefinition,
  ToolRenderResultOptions,
  WorkingIndicatorOptions,
} from '@earendil-works/pi-coding-agent';
import type { Theme as OfficialPiTheme } from '@earendil-works/pi-coding-agent';

// ─── TypeBox ─────────────────────────────────────────────────────────────────

/** Opaque TypeBox schema object produced by Type.Object / Type.String / … */
export type TSchema = Record<string, unknown>;

// ─── Tool result ─────────────────────────────────────────────────────────────

export interface TextContentPart {
  type: 'text';
  text: string;
}

/**
 * Image block for tool results. Pi forwards these to a vision-capable model
 * (shape matches pi-ai's ImageContent: { type:"image", data:<base64>, mimeType }).
 * Pi normalizes/auto-resizes oversized images as they enter history.
 */
export interface ImageContentPart {
  type: 'image';
  data: string;
  mimeType: string;
}

export type ContentPart = TextContentPart | ImageContentPart;

export interface ToolCallResult {
  content: ContentPart[];
  isError?: boolean;
  details?: unknown;
}

export type WorkerLedgerEventType = 'spawned' | 'status' | 'message' | 'tool' | 'handback' | 'exit' | 'error' | 'killed' | 'policy' | 'worktree';

export interface WorkerWorktreeState {
  path: string;
  branch: string;
  baseCommit: string;
  dirtyFiles: number;
  aheadCommits: number;
  mergeState: 'clean' | 'unmerged' | 'conflict' | 'merged' | 'discarded';
}

export interface WorkerLedgerEvent {
  type: WorkerLedgerEventType;
  timestamp: number;
  message?: string;
  details?: unknown;
}

export interface WorkerMessageActivity {
  direction: 'to-agent' | 'from-agent';
  action: 'send' | 'steer' | 'follow-up' | 'reply';
  preview: string;
  timestamp: number;
}

export interface WorkerLedgerEntry {
  agentId: string;
  name: string;
  status: string;
  startedAt: string;
  updatedAt: string;
  model?: string;
  provider?: string;
  /** Original worker assignment, kept separate from transient progress output. */
  task?: string;
  /** Optional parent-plan step this worker was spawned to execute. */
  planStep?: string;
  thinking?: string;
  tools?: string[];
  normalizedStatus?: string;
  result?: string;
  confidence?: string;
  evidence?: string[];
  verification?: string;
  next?: string;
  /** Artifact path explicitly reported by the worker in structured output. */
  artifact?: string;
  /** Parent-assigned durable markdown handback file under .octocode/tmp/agents/<agentId>/. */
  handback?: { path: string; exists: boolean; bytes?: number; modifiedAt?: string };
  /** Rolling 1-line progress note for a running worker (what it is doing now). */
  deltaSummary?: string;
  /** Number of follow-up/steer/send messages queued for the worker but not yet started. */
  pendingMessages?: number;
  /** Latest parent↔worker message, used for a directional footer indicator. */
  lastMessage?: WorkerMessageActivity;
  /** Current running tool, when the worker is inside a tool call. */
  activeTool?: string;
  /** Total tool calls observed for this worker. */
  toolCallCount?: number;
  /** Distinct recent tool names observed for this worker. */
  toolNames?: string[];
  worktree?: WorkerWorktreeState;
  recentEvents: WorkerLedgerEvent[];
}

export interface SpawnPolicy {
  maxActiveAgents: number;
  warningActiveAgents: number;
  requiredPacketSections: string[];
  /**
   * Soft per-worker step (tool-call) budget — a circuit-breaker signal. Research shows
   * multi-agent failure rates climb sharply without per-run budgets; exceeding this surfaces
   * a recovery warning so the parent can abort/steer rather than let a worker run away.
   */
  maxStepsPerWorker: number;
}

export interface SpawnPolicyResult {
  allowed: boolean;
  warnings: string[];
  reason?: string;
}


export interface RenderCallReturn {
  render(width: number): string[];
  invalidate(): void;
}

/**
 * Context object Pi passes as the third argument to renderCall and renderResult.
 * Provides component reuse, cross-slot shared state, and system-level metadata.
 * Optional fields depend on the render slot and execution state.
 * Pi docs: renderCall(args, theme, context) / renderResult(result, opts, theme, context)
 */
export interface RenderContext {
  /** Previously returned component for this slot. Reuse and mutate in place to avoid re-allocation on every streaming frame. */
  lastComponent?: unknown;
  /** Mutable state shared across renderCall and renderResult for the same tool row. */
  state?: Record<string, unknown>;
  /**
   * System-level error flag set by Pi when execute() threw — distinct from result.isError
   * which is the value returned by execute(). Use context.isError in renderResult for
   * reliable error detection (Pi ignores isError in returned ToolCallResult values).
   */
  isError?: boolean;
  /** Request a re-render of this tool row (e.g. after an async state update). */
  invalidate(): void;
  /** Current (possibly partial/streaming) args for this tool call. */
  args?: unknown;
  toolCallId?: string;
  cwd?: string;
  executionStarted?: boolean;
  argsComplete?: boolean;
  isPartial?: boolean;
  expanded?: boolean;
  showImages?: boolean;
}

export interface RenderResultOptions extends Partial<ToolRenderResultOptions> {
  expanded?: boolean;
  isPartial?: boolean;
}

// ─── Pi theme / UI ───────────────────────────────────────────────────────────

export interface PiTheme extends Pick<OfficialPiTheme, 'fg' | 'bold'> {}

export interface PiAutocompleteResult {
  prefix: string;
  items: AutocompleteItem[];
}

export interface PiAutocompleteProvider {
  triggerCharacters?: string[];
  getSuggestions(
    lines: string[], line: number, col: number, options: { signal?: AbortSignal; force?: boolean },
  ): Promise<PiAutocompleteResult | undefined>;
  applyCompletion(lines: string[], line: number, col: number, item: AutocompleteItem, prefix: string): { lines: string[]; cursorLine: number; cursorCol: number };
  shouldTriggerFileCompletion?(lines: string[], line: number, col: number): boolean;
}

export type PiMessageContent = string | ContentPart[];

export interface PiDialogOptions {
  signal?: AbortSignal;
  /** Timeout in milliseconds. */
  timeout?: number;
}

export interface PiUi {
  // Dialogs
  notify?(message: string, level?: 'info' | 'warning' | 'error' | string): void;
  confirm?(title: string, message: string, opts?: PiDialogOptions): Promise<boolean>;
  select?(title: string, items: string[], opts?: PiDialogOptions): Promise<string | undefined>;
  input?(title: string, placeholder?: string, opts?: PiDialogOptions): Promise<string | undefined>;
  editor?(title: string, prefill?: string): Promise<string | undefined>;
  custom?<T>(factory: (tui: unknown, theme: PiTheme, keybindings: unknown, done: (value: T) => void) => unknown, opts?: { overlay?: boolean; overlayOptions?: unknown; onHandle?: (handle: unknown) => void }): Promise<T | undefined>;
  onTerminalInput?(handler: (data: string) => { consume?: boolean; data?: string } | undefined): () => void;
  // Status / widgets
  setHiddenThinkingLabel?(label: string): void;
  setStatus?(name: string, text: string | undefined): void;
  setWidget?(name: string, content: string[] | ((tui: unknown, theme: PiTheme) => unknown) | undefined, opts?: { placement?: 'aboveEditor' | 'belowEditor' }): void;
  setFooter?(factory: ((tui: unknown, theme: PiTheme, footerData?: ReadonlyFooterDataProvider) => unknown) | undefined): void;
  setHeader?(factory: ((tui: unknown, theme: PiTheme) => unknown) | undefined): void;
  setTitle?(title: string): void;
  setWorkingMessage?(message?: string): void;
  setWorkingVisible?(visible: boolean): void;
  setWorkingIndicator?(indicator?: WorkingIndicatorOptions): void;
  // Editor
  setEditorText?(text: string): void;
  getEditorText?(): string;
  pasteToEditor?(text: string): void;
  setEditorComponent?(factory: ((tui: unknown, theme: PiTheme, keybindings: unknown) => unknown) | undefined): void;
  getEditorComponent?(): ((tui: unknown, theme: PiTheme, keybindings: unknown) => unknown) | undefined;
  addAutocompleteProvider?(provider: (current: PiAutocompleteProvider) => PiAutocompleteProvider): void;
  // Tool display
  getToolsExpanded?(): boolean;
  setToolsExpanded?(expanded: boolean): void;
  // Themes
  getAllThemes?(): Array<{ name: string; path: string | undefined }>;
  getTheme?(name: string): unknown;
  setTheme?(nameOrTheme: string | unknown): { success: boolean; error?: string };
  theme?: PiTheme;
}

// ─── Pi context ──────────────────────────────────────────────────────────────

export interface PiSessionManager {
  getCwd?(): string | undefined;
  getSessionDir?(): string | undefined;
  getSessionId?(): string | undefined;
  getSessionFile?(): string | undefined;
  getSessionName?(): string | undefined;
  getLeafId?(): string | undefined;
  getLeafEntry?(): unknown | undefined;
  getEntry?(entryId: string): unknown | undefined;
  getLabel?(entryId: string): string | undefined;
  /** Session entries root→leaf (mirrors Pi's ReadonlySessionManager.getBranch). Entries carry a `type` discriminant, e.g. 'compaction'. */
  getBranch?(): unknown[];
  getHeader?(): unknown | undefined;
  getEntries?(): unknown[];
  getTree?(): unknown[];
}

export interface CompactOptions {
  customInstructions?: string;
  /** Called after compaction completes. Pi passes an opaque result object. */
  onComplete?(result?: unknown): void;
  onError?(err: Error): void;
}

export interface NewSessionOptions {
  parentSession?: string | undefined;
  setup?(sm: PiSessionManager): void;
  /** Receives ReplacedSessionContext (extends ExtensionCommandContext) — typed here as PiCommandContext. */
  withSession?(ctx: PiCommandContext): Promise<void>;
}

export interface PiModel {
  id?: string;
  reasoning?: boolean;
  /** Provider transport used by Pi's model adapter. */
  api?: string;
  /** Provider-specific request compatibility controls. */
  compat?: {
    forceAdaptiveThinking?: boolean;
    [key: string]: unknown;
  };
  /** Provider name for this model (e.g. "anthropic", "guy-provider-anthropic"). Present on the active model. */
  provider?: string;
}

/**
 * Context available inside tool execute() handlers and event handlers.
 * Does NOT include session-control methods (newSession, reload) — those
 * are only available in ExtensionCommandContext.
 */
export interface PiContext {
  cwd?: string;
  ui?: PiUi;
  model?: PiModel;
  hasUI?: boolean;
  /** 'tui' = interactive terminal, 'rpc' = JSON RPC, 'json' = event stream, 'print' = -p flag */
  mode?: 'tui' | 'rpc' | 'json' | 'print';
  /** Pi reports project trust synchronously. */
  isProjectTrusted?(): boolean;
  compact?(opts: CompactOptions): void;
  /** `tokens` is null when unknown — e.g. right after compaction (mirrors Pi's ContextUsage). */
  getContextUsage?(): (Partial<PiContextUsage> & { tokens: number | null; contextWindow: number }) | null | undefined;
  sessionManager?: PiSessionManager;
  modelRegistry?: {
    find(provider: string, id: string): PiModel | undefined;
    complete?(
      model: PiModel,
      context: { systemPrompt?: string; messages: Array<{ role: 'user'; content: string; timestamp: number }> },
      options?: { signal?: AbortSignal },
    ): Promise<unknown>;
  };
}

/**
 * Extended context available inside command handlers (registerCommand).
 * Adds session-control methods that MUST NOT be called from tool execute()
 * or event handlers — they can deadlock in those contexts.
 */
export interface PiSendUserMessageOptions {
  deliverAs?: 'steer' | 'followUp' | string;
  /** Dispatch extension slash commands and expand skill/prompt templates when true. */
  expandPromptTemplates?: boolean;
}

export interface PiCommandContext extends PiContext {
  newSession?(opts?: NewSessionOptions): Promise<{ cancelled?: boolean } | undefined>;
  /** Fire-and-forget user message (available in commands and in withSession callbacks). */
  sendUserMessage?(content: PiMessageContent, opts?: PiSendUserMessageOptions): void | Promise<void>;
  reload?(): Promise<void>;
  /** Fork the session at an entry into a new session file (mirrors Pi's ExtensionCommandContext.fork). */
  fork?(entryId: string, opts?: { position?: 'before' | 'at'; withSession?: (ctx: PiCommandContext) => Promise<void> }): Promise<{ cancelled?: boolean }>;
  /** Programmatic rewind: move the session leaf to targetId in place (mirrors Pi's navigateTree). */
  navigateTree?(targetId: string, opts?: { summarize?: boolean; customInstructions?: string; replaceInstructions?: boolean; label?: string }): Promise<{ cancelled?: boolean }>;
  switchSession?(sessionPath: string, opts?: { withSession?: (ctx: PiCommandContext) => Promise<void> }): Promise<{ cancelled?: boolean }>;
  waitForIdle?(): Promise<void>;
}

// ─── Pi tool ─────────────────────────────────────────────────────────────────

export interface ToolDefinition extends Partial<Omit<PiToolDefinition<any, unknown, any>, 'parameters' | 'prepareArguments' | 'execute' | 'renderCall' | 'renderResult'>> {
  name: string;
  label: string;
  description: string;
  promptSnippet?: string;
  promptGuidelines?: string[];
  parameters: TSchema;
  /** Optional pre-validation argument normalizer. */
  prepareArguments?(args: unknown): unknown;
  execute(
    toolCallId: string,
    params: Record<string, unknown>,
    signal?: AbortSignal,
    onUpdate?: unknown,
    ctx?: PiContext,
  ): Promise<ToolCallResult>;
  renderCall?(args: unknown, theme?: PiTheme, context?: RenderContext): RenderCallReturn;
  renderResult?(result: ToolCallResult, opts: RenderResultOptions, theme?: PiTheme, context?: RenderContext): RenderCallReturn;
}

// ─── Pi command ──────────────────────────────────────────────────────────────

export interface AutocompleteItem {
  value: string;
  label: string;
  description?: string;
}

export interface CommandDefinition {
  description: string;
  handler(args: string, ctx: PiCommandContext): Promise<void>;
  /** Optional tab-completion for command arguments in TUI interactive mode. */
  getArgumentCompletions?(prefix: string): AutocompleteItem[] | null | Promise<AutocompleteItem[] | null>;
}

// ─── Pi event payloads ───────────────────────────────────────────────────────

export interface ThinkingLevelEvent {
  level?: string;
}

/** A skill entry as Pi exposes it in systemPromptOptions.skills. */
export interface SkillInfo {
  name: string;
  description?: string;
  path?: string;
  source?: string;
  scope?: string;
}

/**
 * Structured inputs Pi uses to build the system prompt for each turn.
 * Accessible via event.systemPromptOptions in before_agent_start.
 * Use to inspect loaded skills, active tools, or custom guidelines without
 * re-parsing the rendered systemPrompt string.
 */
export interface BuildSystemPromptOptions extends Partial<Omit<PiBuildSystemPromptOptions, 'skills' | 'contextFiles'>> {
  customPrompt?: string;
  selectedTools?: string[];
  toolSnippets?: Record<string, string>;
  promptGuidelines?: string[];
  appendSystemPrompt?: string;
  cwd?: string;
  contextFiles?: unknown[];
  skills?: SkillInfo[];
}

export interface BeforeAgentStartEvent {
  systemPrompt: string;
  /** Structured prompt inputs — inspect loaded skills, active tools, and guidelines. */
  systemPromptOptions?: BuildSystemPromptOptions;
}

export interface ResourcesDiscoverResult {
  skillPaths?: string[];
}

export interface BeforeAgentStartResult {
  systemPrompt?: string;
}

// ─── Pi instance ─────────────────────────────────────────────────────────────

export interface TurnEndEvent {
  turnIndex?: number;
  message?: {
    stopReason?: string;
    usage?: {
      input?: number;
      output?: number;
      cacheRead?: number;
      cacheWrite?: number;
      totalTokens?: number;
    };
  };
}

export interface SessionBeforeCompactEvent {
  preparation: unknown;
  branchEntries?: unknown[];
  customInstructions?: string;
  reason: 'manual' | 'threshold' | 'overflow' | string;
  willRetry: boolean;
  signal: AbortSignal;
}

export interface SessionCompactEvent {
  compactionEntry: unknown;
  fromExtension: boolean;
  reason: 'manual' | 'threshold' | 'overflow' | string;
  willRetry: boolean;
}

export interface SessionCompactFailedEvent {
  reason: 'manual' | 'threshold' | 'overflow' | string;
  errorMessage?: string;
  aborted: boolean;
  willRetry: boolean;
  fromExtension: boolean;
}

export interface SessionShutdownEvent {
  reason?: 'quit' | 'reload' | 'new' | 'resume' | 'fork';
}

export interface PiExecResult {
  stdout: string;
  stderr: string;
  code: number | null;
  killed?: boolean;
}

export interface PiCommand {
  name: string;
  description?: string;
  source: 'extension' | 'prompt' | 'skill';
  sourceInfo: {
  path: string;
    source: string;
    scope: 'user' | 'project' | 'temporary';
    origin: 'package' | 'top-level';
    baseDir?: string;
  };
}

export interface PiToolMeta {
  name: string;
  description: string;
  parameters: unknown;
  promptGuidelines?: string[];
  sourceInfo: { path: string; source: string; scope: string; origin: string };
}

export interface PiInstance {
  // ─── Events ─────────────────────────────────────────────────────────────────
  on(event: 'session_start', handler: (event: { reason: string; previousSessionFile?: string }, ctx: PiContext) => Promise<void>): void;
  on(event: 'session_shutdown', handler: (event: SessionShutdownEvent, ctx: PiContext) => Promise<void>): void;
  on(event: 'session_info_changed', handler: (event: { name?: string }, ctx: PiContext) => Promise<void>): void;
  on(event: 'session_before_switch', handler: (event: { reason: 'new' | 'resume'; targetSessionFile?: string }, ctx: PiContext) => Promise<{ cancel?: boolean } | void>): void;
  on(event: 'session_before_fork', handler: (event: { entryId: string; position: string }, ctx: PiContext) => Promise<{ cancel?: boolean } | void>): void;
  on(event: 'session_before_compact', handler: (event: SessionBeforeCompactEvent, ctx: PiContext) => Promise<{ cancel?: boolean; compaction?: unknown } | void>): void;
  on(event: 'session_compact', handler: (event: SessionCompactEvent, ctx: PiContext) => Promise<void>): void;
  on(event: 'session_compact_failed', handler: (event: SessionCompactFailedEvent, ctx: PiContext) => Promise<void>): void;
  /** After /tree navigation lands on a new leaf (mirrors Pi's session_tree). */
  on(event: 'session_tree', handler: (event: { newLeafId?: string; oldLeafId?: string }, ctx: PiContext) => Promise<void>): void;
  on(event: 'model_select', handler: (event: { model: PiModel; previousModel?: PiModel; source: string }, ctx: PiContext) => Promise<void>): void;
  on(event: 'thinking_level_select', handler: (event: ThinkingLevelEvent, ctx: PiContext) => Promise<void>): void;
  on(event: 'before_agent_start', handler: (event: BeforeAgentStartEvent, ctx?: PiContext) => Promise<BeforeAgentStartResult | void>): void;
  on(event: 'agent_start', handler: (event: unknown, ctx: PiContext) => Promise<void>): void;
  on(event: 'agent_end', handler: (event: { messages: unknown[] }, ctx: PiContext) => Promise<void>): void;
  on(event: 'turn_start', handler: (event: { turnIndex: number; timestamp: number }, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'turn_end', handler: (event: TurnEndEvent, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'resources_discover', handler: (event: { cwd: string; reason: string }, ctx: PiContext) => Promise<ResourcesDiscoverResult | void>): void;
  on(event: 'project_trust', handler: (event: { cwd: string }, ctx: PiContext) => Promise<{ trusted: 'yes' | 'no' | 'undecided'; remember?: boolean }>): void;
  on(event: 'context', handler: (event: { messages: unknown[] }, ctx: PiContext) => Promise<{ messages: unknown[] } | void>): void;
  on(event: 'input', handler: (event: { text: string; images?: unknown[]; source?: 'interactive' | 'rpc' | 'extension'; streamingBehavior?: 'steer' | 'followUp' }, ctx: PiContext) => Promise<{ action?: 'continue' | 'transform' | 'handled'; text?: string; images?: unknown[] } | void>): void;
  on(event: 'message_start', handler: (event: { message: unknown }, ctx: PiContext) => Promise<void>): void;
  on(event: 'message_end', handler: (event: { message: unknown }, ctx: PiContext) => Promise<{ message?: unknown } | void>): void;
  on(event: 'tool_call', handler: (event: { toolCallId: string; toolName: string; input: Record<string, unknown> }, ctx: PiContext) => Promise<{ block?: boolean; reason?: string } | void>): void;
  on(event: 'tool_execution_start', handler: (event: { toolCallId: string; toolName: string; args: unknown }, ctx: PiContext) => Promise<void>): void;
  on(event: 'tool_execution_end', handler: (event: { toolCallId: string; toolName: string; result: unknown; isError: boolean }, ctx: PiContext) => Promise<void>): void;
  on(event: 'before_provider_request', handler: (event: { payload: unknown }, ctx: PiContext) => unknown): void;
  on(event: 'after_provider_response', handler: (event: { status: number; headers: Record<string, string> }, ctx: PiContext) => void): void;
  // Additional Pi-published events. Not currently subscribed by the extension,
  // but declared so their names autocomplete and typos don't fall through to the
  // untyped catch-all below. Payloads are left as unknown pending a concrete need.
  on(event: 'agent_settled', handler: (event: unknown, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'before_provider_headers', handler: (event: unknown, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'message_update', handler: (event: unknown, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'session_before_tree', handler: (event: unknown, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'tool_execution_update', handler: (event: unknown, ctx: PiContext) => void | Promise<void>): void;
  on(event: 'tool_result', handler: (event: unknown, ctx: PiContext) => void | Promise<void | Partial<ToolCallResult>>): void;
  on(event: 'user_bash', handler: (event: unknown, ctx: PiContext) => void | Promise<void>): void;
  on(event: string, handler: (...args: unknown[]) => unknown): void;
  // ─── Tools ──────────────────────────────────────────────────────────────────
  registerTool?(definition: ToolDefinition): void;
  getActiveTools?(): string[];
  getAllTools?(): PiToolMeta[];
  setActiveTools?(tools: string[]): void;
  // ─── Commands ───────────────────────────────────────────────────────────────
  registerCommand?(name: string, opts: CommandDefinition): void;
  getCommands?(): PiCommand[];
  // ─── Shortcuts / flags ──────────────────────────────────────────────────────
  registerShortcut?(shortcut: string, opts: { description: string; handler: (ctx: PiContext) => Promise<void> }): void;
  registerFlag?(name: string, opts: { description: string; type: 'boolean' | 'string'; default?: unknown }): void;
  getFlag?(name: string): unknown;
  // ─── Messages / events bus ──────────────────────────────────────────────────
  sendUserMessage(content: PiMessageContent, opts?: PiSendUserMessageOptions): void;
  sendMessage?(msg: { customType: string; content: PiMessageContent; display?: boolean; details?: unknown }, opts?: { triggerTurn?: boolean; deliverAs?: 'steer' | 'followUp' | 'nextTurn' | string }): void;
  registerMessageRenderer?(customType: string, renderer: (message: unknown, options: { expanded: boolean }, theme: PiTheme) => unknown): void;
  events?: { on(event: string, cb: (data: unknown) => void): void; emit(event: string, data: unknown): void };
  // ─── Model / thinking ───────────────────────────────────────────────────────
  getThinkingLevel?(): string | undefined;
  setThinkingLevel?(level: string): void;
  setModel?(model: PiModel): Promise<boolean>;
  // ─── Session / labels ───────────────────────────────────────────────────────
  setSessionName?(name: string): void;
  getSessionName?(): string | undefined;
  /**
   * Persist a CustomEntry — never in LLM context. Renders in the transcript
   * when paired with registerEntryRenderer (pi docs §appendEntry); otherwise
   * state-only.
   */
  appendEntry?(customType: string, data?: unknown): void;
  /** Render a CustomEntry type in the transcript (durable, TUI-only, zero prompt cost). */
  registerEntryRenderer?(customType: string, renderer: (entry: { data?: unknown }, options: { expanded: boolean }, theme: PiTheme) => unknown): void;
  setLabel?(entryId: string, label: string | undefined): void;
  // ─── Providers ──────────────────────────────────────────────────────────────
  registerProvider?(name: string, config: Record<string, unknown>): void;
  unregisterProvider?(name: string): void;
  // ─── Shell ──────────────────────────────────────────────────────────────────
  exec?(command: string, args: string[], opts?: { signal?: AbortSignal; timeout?: number }): Promise<PiExecResult>;
}

// ─── Extension options ───────────────────────────────────────────────────────

export interface OctocodePiExtensionOptions {
  promptMode?: PromptMode;
  /**
   * Explicit Pi host version for embedders that isolate peer package metadata.
   * Normal Pi activation resolves the installed host package version instead.
   */
  hostVersion?: string;
}

/**
 * Canonical notify-callback signature used by tool modules that surface messages
 * through the host UI. Single source of truth so the contract can't drift across
 * modules (previously redefined independently in mcp-tool and plan-tool).
 */
export type NotifyFn = (ctx: PiContext | undefined, message: string, level?: string) => void;
