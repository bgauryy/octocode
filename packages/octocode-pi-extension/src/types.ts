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

export interface RenderResultOptions {
  expanded?: boolean;
  isPartial?: boolean;
}

// ─── Pi theme / UI ───────────────────────────────────────────────────────────

export interface PiTheme {
  fg(color: string, text: string): string;
  bold(text: string): string;
}

export interface PiUi {
  notify?(message: string, level?: string): void;
  confirm?(title: string, message: string): Promise<boolean>;
  setHiddenThinkingLabel?(label: string): void;
  setStatus?(name: string, text: string): void;
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
  dbPath?: string;
  hasUI?: boolean;
  isProjectTrusted?(): Promise<boolean>;
  compact?(opts: CompactOptions): void;
  getContextUsage?(): { tokens: number; contextWindow: number } | null | undefined;
  sessionManager?: PiSessionManager;
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
  renderCall?(args: unknown, theme?: PiTheme): RenderCallReturn;
  renderResult?(result: ToolCallResult, opts: RenderResultOptions, theme?: PiTheme): RenderCallReturn;
}

// ─── Pi command ──────────────────────────────────────────────────────────────

export interface CommandDefinition {
  description: string;
  handler(args: string, ctx: PiCommandContext): Promise<void>;
}

// ─── Pi event payloads ───────────────────────────────────────────────────────

export interface ThinkingLevelEvent {
  level?: string;
}

export interface BeforeAgentStartEvent {
  systemPrompt: string;
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

export interface PiInstance {
  on(event: 'session_start', handler: (event: unknown, ctx: PiContext) => Promise<void>): void;
  on(event: 'session_shutdown', handler: (event: SessionShutdownEvent, ctx: PiContext) => Promise<void>): void;
  on(event: 'model_select', handler: (event: unknown, ctx: PiContext) => Promise<void>): void;
  on(
    event: 'thinking_level_select',
    handler: (event: ThinkingLevelEvent, ctx: PiContext) => Promise<void>,
  ): void;
  on(
    event: 'before_agent_start',
    handler: (event: BeforeAgentStartEvent) => Promise<BeforeAgentStartResult | void>,
  ): void;
  on(
    event: 'resources_discover',
    handler: () => Promise<ResourcesDiscoverResult>,
  ): void;
  on(event: 'turn_end', handler: (event: TurnEndEvent, ctx: PiContext) => void | Promise<void>): void;
  on(event: string, handler: (...args: unknown[]) => unknown): void;
  registerTool?(definition: ToolDefinition): void;
  registerCommand?(name: string, opts: CommandDefinition): void;
  sendUserMessage(text: string, opts?: { deliverAs?: string }): void;
  getActiveTools?(): string[];
  setActiveTools?(tools: string[]): void;
  getThinkingLevel?(): string | undefined;
}

// ─── Extension options ───────────────────────────────────────────────────────

export type PromptMode = 'append' | 'replace';

export interface OctocodePiExtensionOptions {
  promptMode?: PromptMode;
}
