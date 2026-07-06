/**
 * Pi runtime API type definitions.
 *
 * These are not published by Pi — defined here against the live API contract
 * observed in the codebase. Use `skipLibCheck: true` for flexibility.
 */

// ─── TypeBox ─────────────────────────────────────────────────────────────────

/** Opaque TypeBox schema object produced by Type.Object / Type.String / … */
export type TSchema = Record<string, unknown>;

// ─── Tool result ─────────────────────────────────────────────────────────────

export interface ContentPart {
  type: 'text';
  text: string;
}

export interface ToolCallResult {
  content: ContentPart[];
  isError?: boolean;
  details?: unknown;
}

export interface RenderCallReturn {
  render(width: number): string[];
  invalidate(): void;
}

/**
 * Context object Pi passes as the third argument to renderCall and renderResult.
 * Provides component reuse, cross-slot shared state, and system-level metadata.
 * All fields are optional — not all Pi versions expose every field.
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

export interface RenderResultOptions {
  expanded?: boolean;
  isPartial?: boolean;
}

// ─── Pi theme / UI ───────────────────────────────────────────────────────────

export interface PiTheme {
  fg(color: string, text: string): string;
  bold(text: string): string;
}

export interface PiAutocompleteItem {
  value: string;
  label: string;
  description?: string;
}

export interface PiAutocompleteResult {
  prefix: string;
  items: PiAutocompleteItem[];
}

export interface PiAutocompleteProvider {
  triggerCharacters?: string[];
  getSuggestions(
    lines: string[], line: number, col: number, options: unknown,
  ): Promise<PiAutocompleteResult | undefined>;
  applyCompletion(lines: string[], line: number, col: number, item: PiAutocompleteItem, prefix: string): unknown;
  shouldTriggerFileCompletion?(lines: string[], line: number, col: number): boolean;
}

export interface PiWorkingIndicator {
  frames: string[];
  intervalMs?: number;
}

export interface PiUi {
  // Dialogs
  notify?(message: string, level?: string): void;
  confirm?(title: string, message: string, opts?: { timeout?: number; signal?: AbortSignal }): Promise<boolean>;
  select?(title: string, items: string[] | PiAutocompleteItem[], opts?: { timeout?: number }): Promise<string | undefined>;
  input?(title: string, placeholder?: string, opts?: { timeout?: number }): Promise<string | undefined>;
  editor?(title: string, prefill?: string): Promise<string | undefined>;
  custom?<T>(factory: (tui: unknown, theme: PiTheme, keybindings: unknown, done: (value: T) => void) => unknown, opts?: { overlay?: boolean; overlayOptions?: unknown; onHandle?: (handle: unknown) => void }): Promise<T | undefined>;
  // Status / widgets
  setHiddenThinkingLabel?(label: string): void;
  setStatus?(name: string, text: string | undefined): void;
  setWidget?(name: string, content: string[] | ((tui: unknown, theme: PiTheme) => unknown) | undefined, opts?: { placement?: 'aboveEditor' | 'belowEditor' }): void;
  setFooter?(factory: ((tui: unknown, theme: PiTheme) => unknown) | undefined): void;
  setHeader?(factory: ((tui: unknown, theme: PiTheme) => unknown) | undefined): void;
  setTitle?(title: string): void;
  setWorkingMessage?(message?: string): void;
  setWorkingVisible?(visible: boolean): void;
  setWorkingIndicator?(indicator?: PiWorkingIndicator): void;
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
  getSessionFile?(): string | undefined;
  appendMessage?(opts: unknown): void;
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
  /** Path to the Octocode awareness SQLite DB injected by the extension. */
  dbPath?: string;
  isProjectTrusted?(): Promise<boolean>;
  compact?(opts: CompactOptions): void;
  getContextUsage?(): { tokens: number; contextWindow: number } | null | undefined;
  sessionManager?: PiSessionManager;
  modelRegistry?: {
    find(provider: string, id: string): PiModel | undefined;
  };
}

/**
 * Extended context available inside command handlers (registerCommand).
 * Adds session-control methods that MUST NOT be called from tool execute()
 * or event handlers — they can deadlock in those contexts.
 */
export interface PiCommandContext extends PiContext {
  newSession?(opts?: NewSessionOptions): Promise<{ cancelled?: boolean } | undefined>;
  /** Fire-and-forget user message (available in commands and in withSession callbacks). */
  sendUserMessage?(text: string, opts?: { deliverAs?: string }): void | Promise<void>;
  reload?(): Promise<void>;
}

// ─── Pi tool ─────────────────────────────────────────────────────────────────

export interface ToolDefinition {
  name: string;
  label: string;
  description: string;
  promptSnippet?: string;
  promptGuidelines?: string[];
  parameters: TSchema;
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
  getArgumentCompletions?(prefix: string): AutocompleteItem[] | null;
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
export interface BuildSystemPromptOptions {
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
  on(event: 'session_before_compact', handler: (event: { preparation: unknown; reason: string; willRetry: boolean; signal: AbortSignal }, ctx: PiContext) => Promise<{ cancel?: boolean; compaction?: unknown } | void>): void;
  on(event: 'session_compact', handler: (event: { compactionEntry: unknown; fromExtension: boolean; reason: string; willRetry: boolean }, ctx: PiContext) => Promise<void>): void;
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
  on(event: 'input', handler: (event: { text: string; images: unknown[] }, ctx: PiContext) => Promise<{ text?: string; handled?: boolean } | void>): void;
  on(event: 'message_start', handler: (event: { message: unknown }, ctx: PiContext) => Promise<void>): void;
  on(event: 'message_end', handler: (event: { message: unknown }, ctx: PiContext) => Promise<{ message?: unknown } | void>): void;
  on(event: 'tool_call', handler: (event: { toolCallId: string; toolName: string; input: Record<string, unknown> }, ctx: PiContext) => Promise<{ block?: boolean; reason?: string } | void>): void;
  on(event: 'tool_execution_start', handler: (event: { toolCallId: string; toolName: string; args: unknown }, ctx: PiContext) => Promise<void>): void;
  on(event: 'tool_execution_end', handler: (event: { toolCallId: string; toolName: string; result: unknown; isError: boolean }, ctx: PiContext) => Promise<void>): void;
  on(event: 'before_provider_request', handler: (event: { payload: unknown }, ctx: PiContext) => unknown): void;
  on(event: 'after_provider_response', handler: (event: { status: number; headers: Record<string, string> }, ctx: PiContext) => void): void;
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
  sendUserMessage(text: string, opts?: { deliverAs?: string }): void;
  sendMessage?(msg: { customType: string; content: string; display?: boolean; details?: unknown }): void;
  registerMessageRenderer?(customType: string, renderer: (message: unknown, options: { expanded?: boolean }, theme: PiTheme) => unknown): void;
  events?: { on(event: string, cb: (data: unknown) => void): void; emit(event: string, data: unknown): void };
  // ─── Model / thinking ───────────────────────────────────────────────────────
  getThinkingLevel?(): string | undefined;
  setThinkingLevel?(level: string): void;
  setModel?(model: PiModel): Promise<boolean>;
  // ─── Session / labels ───────────────────────────────────────────────────────
  setSessionName?(name: string): void;
  getSessionName?(): string | undefined;
  appendEntry?(entry: unknown): void;
  setLabel?(entryId: string, label: string | undefined): void;
  // ─── Providers ──────────────────────────────────────────────────────────────
  registerProvider?(name: string, config: Record<string, unknown>): void;
  unregisterProvider?(name: string): void;
  // ─── Shell ──────────────────────────────────────────────────────────────────
  exec?(command: string, args: string[], opts?: { signal?: AbortSignal; timeout?: number }): Promise<PiExecResult>;
}

// ─── Extension options ───────────────────────────────────────────────────────

export type PromptMode = 'append' | 'octocode-first' | 'replace';

export interface OctocodePiExtensionOptions {
  promptMode?: PromptMode;
}
