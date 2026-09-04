import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  AgentSessionRuntime,
  createAgentSession,
  DefaultResourceLoader,
  RpcClient,
  runPrintMode,
  SessionManager,
  SettingsManager,
  type ExtensionFactory,
} from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import type { PiInstance } from "../types.js";

const PROVIDER = "octocode-production-probe";
const MODEL = "deterministic-v1";
const API = "octocode-production-probe-api";
const TOOL = "productionProbeTool";
const MAX_EVENTS = 256;
const MAX_EFFECTS = 64;
const MAX_OBSERVATIONS = 64;
const REDACT = /secret|token|api[_-]?key|authorization|password|cookie/i;

export const PRODUCTION_PI_SCENARIO_IDS = [
  "deterministic-model-turn",
  "streaming-tool-flow",
  "policy-denial-matrix",
  "tool-failure-matrix",
  "cancellation-boundaries",
  "steer-and-follow-up",
  "session-lifecycle",
  "compaction-matrix",
  "ui-semantics",
  "transport-corpus",
  "persistence-restart",
  "codex-hook-lifecycle",
  "plugin-lifecycle",
] as const;
export type ProductionPiScenarioId =
  (typeof PRODUCTION_PI_SCENARIO_IDS)[number];
export interface ProductionPiScenarioInput {
  readonly scenario: Readonly<{ readonly id: ProductionPiScenarioId | string }>;
  readonly signal: AbortSignal;
}
export interface ProductionPiScenarioReceipt {
  readonly source: "installed-pi-sdk";
  readonly events: readonly {
    readonly kind: string;
    readonly data?: unknown;
  }[];
  readonly effects: readonly {
    readonly id: string;
    readonly kind: string;
    readonly effectful: boolean;
    readonly data?: unknown;
  }[];
  /** Host-specific evidence retained outside semantic parity comparison. */
  readonly observations?: readonly {
    readonly kind: string;
    readonly data?: unknown;
  }[];
}
export type ProductionPiScenarioProbe = (
  input: ProductionPiScenarioInput,
) => Promise<ProductionPiScenarioReceipt>;
export interface ProductionPiScenarioSuite {
  readonly scenarioProbes: Readonly<
    Partial<Record<ProductionPiScenarioId, ProductionPiScenarioProbe>>
  >;
  readonly unsupportedReasons: Readonly<
    Partial<Record<ProductionPiScenarioId, string>>
  >;
}
export interface ProductionPiLifecycleCapture {
  readonly started: boolean;
  readonly stopped: boolean;
  readonly registry: Readonly<{
    tools: readonly string[];
    commands: readonly string[];
    hooks: readonly string[];
  }>;
}

type Block =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string }
  | {
      type: "toolCall";
      id: string;
      name: string;
      arguments: Record<string, unknown>;
    };
interface Response {
  content: Block[];
  stopReason: "stop" | "toolUse" | "length" | "error";
  delayMs?: number;
  startDelayMs?: number;
  errorMessage?: string;
  usage?: Readonly<{
    input: number;
    output: number;
    cacheRead?: number;
    cacheWrite?: number;
  }>;
}
interface Event {
  kind: string;
  data?: unknown;
}
interface Effect {
  id: string;
  kind: string;
  effectful: boolean;
  data?: unknown;
}

class EventStream {
  readonly queue: unknown[] = [];
  readonly waiting: ((value: IteratorResult<unknown>) => void)[] = [];
  readonly final: Promise<unknown>;
  resolveFinal!: (value: unknown) => void;
  done = false;
  constructor() {
    this.final = new Promise((resolve) => {
      this.resolveFinal = resolve;
    });
  }
  push(event: unknown): void {
    if (this.done) return;
    const value = record(event);
    if (value?.type === "done" || value?.type === "error") {
      this.done = true;
      this.resolveFinal(value.type === "done" ? value.message : value.error);
    }
    const waiter = this.waiting.shift();
    if (waiter) waiter({ value: event, done: false });
    else this.queue.push(event);
  }
  end(result?: unknown): void {
    this.done = true;
    if (result !== undefined) this.resolveFinal(result);
    for (const waiter of this.waiting.splice(0))
      waiter({ value: undefined, done: true });
  }
  async *[Symbol.asyncIterator](): AsyncIterator<unknown> {
    while (true) {
      if (this.queue.length) {
        yield this.queue.shift();
        continue;
      }
      if (this.done) return;
      const next = await new Promise<IteratorResult<unknown>>((resolve) =>
        this.waiting.push(resolve),
      );
      if (next.done) return;
      yield next.value;
    }
  }
  result(): Promise<unknown> {
    return this.final;
  }
}

function record(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}
function message(response: Response): Record<string, unknown> {
  const input = response.usage?.input ?? 1;
  const output = response.usage?.output ?? 1;
  const cacheRead = response.usage?.cacheRead ?? 0;
  const cacheWrite = response.usage?.cacheWrite ?? 0;
  return {
    role: "assistant",
    content: response.content.map((part) => ({ ...part })),
    api: API,
    provider: PROVIDER,
    model: MODEL,
    usage: {
      input,
      output,
      cacheRead,
      cacheWrite,
      totalTokens: input + output + cacheRead + cacheWrite,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    },
    stopReason: response.stopReason,
    ...(response.errorMessage ? { errorMessage: response.errorMessage } : {}),
    timestamp: Date.now(),
  };
}
async function wait(ms: number, signal?: AbortSignal): Promise<void> {
  if (!ms) return;
  await new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}
function stream(
  response: Response,
  signal: AbortSignal | undefined,
  started: () => void,
): EventStream {
  const output = new EventStream();
  queueMicrotask(async () => {
    await wait(response.startDelayMs ?? 0, signal);
    const final = message(response);
    const partial = { ...final, content: [], stopReason: "pending" };
    output.push({ type: "start", partial });
    for (let index = 0; index < response.content.length; index++) {
      const block = response.content[index]!;
      const prefix = block.type === "toolCall" ? "toolcall" : block.type;
      output.push({ type: `${prefix}_start`, contentIndex: index, partial });
      started();
      await wait(response.delayMs ?? 0, signal);
      if (signal?.aborted) {
        const aborted = {
          ...partial,
          stopReason: "aborted",
          errorMessage: "Request was aborted",
        };
        output.push({ type: "error", reason: "aborted", error: aborted });
        output.end(aborted);
        return;
      }
      if (block.type === "text") {
        output.push({
          type: "text_delta",
          contentIndex: index,
          delta: block.text,
          partial,
        });
        output.push({
          type: "text_end",
          contentIndex: index,
          content: block.text,
          partial,
        });
      } else if (block.type === "thinking") {
        output.push({
          type: "thinking_delta",
          contentIndex: index,
          delta: block.thinking,
          partial,
        });
        output.push({
          type: "thinking_end",
          contentIndex: index,
          content: block.thinking,
          partial,
        });
      } else {
        output.push({
          type: "toolcall_delta",
          contentIndex: index,
          delta: JSON.stringify(block.arguments),
          partial,
        });
        output.push({
          type: "toolcall_end",
          contentIndex: index,
          toolCall: block,
          partial,
        });
      }
    }
    output.push({ type: "done", reason: response.stopReason, message: final });
    output.end(final);
  });
  return output;
}
function userTexts(context: unknown): string[] {
  const messages = record(context)?.messages;
  if (!Array.isArray(messages)) return [];
  return messages.flatMap((item) => {
    const value = record(item);
    if (value?.role !== "user") return [];
    if (typeof value.content === "string") return [value.content];
    if (!Array.isArray(value.content)) return [];
    return value.content.flatMap((part) => {
      const content = record(part);
      return content?.type === "text" && typeof content.text === "string"
        ? [content.text]
        : [];
    });
  });
}
function capture(event: unknown, events: Event[], effects: Effect[]): void {
  const value = record(event);
  if (!value || typeof value.type !== "string") return;
  if (value.type === "agent_start") events.push({ kind: "turn.started" });
  if (value.type === "agent_end") events.push({ kind: "turn.completed" });
  if (value.type === "message_update") {
    const update = record(value.assistantMessageEvent);
    const kinds: Record<string, string> = {
      text_delta: "stream.text",
      thinking_delta: "stream.thinking",
      toolcall_delta: "stream.tool-arguments",
    };
    if (typeof update?.type === "string" && kinds[update.type]) {
      events.push({ kind: kinds[update.type]!, data: { delta: update.delta } });
    }
  }
  if (value.type === "tool_execution_update") {
    events.push({
      kind: "stream.tool-update",
      data: { callId: value.toolCallId, tool: value.toolName },
    });
  }
  if (value.type === "tool_execution_end") {
    events.push({
      kind: "stream.tool-result",
      data: {
        callId: value.toolCallId,
        tool: value.toolName,
        isError: value.isError === true,
      },
    });
    effects.push({
      id: `streaming-tool-flow:${String(value.toolCallId ?? "tool")}`,
      kind: "tool.execution",
      effectful: false,
      data: { tool: value.toolName },
    });
  }
  if (value.type === "compaction_start") {
    events.push({ kind: "compaction.started", data: { reason: value.reason } });
  }
  if (value.type === "compaction_end") {
    const data = {
      reason: value.reason,
      aborted: value.aborted === true,
      willRetry: value.willRetry === true,
      persisted: record(value.result)?.summary !== undefined,
      ...(typeof value.errorMessage === "string"
        ? { error: value.errorMessage }
        : {}),
    };
    if (value.aborted === true)
      events.push({ kind: "compaction.cancelled", data });
    else if (value.result !== undefined)
      events.push({ kind: "compaction.completed", data });
    else if (
      value.reason === "overflow" &&
      typeof value.errorMessage === "string"
    ) {
      events.push({ kind: "compaction.failed-retry", data });
    } else events.push({ kind: "compaction.failed", data });
  }
}
function clean(value: unknown, depth = 0, key = ""): unknown {
  if (REDACT.test(key)) return "[REDACTED]";
  if (depth > 6) return "[TRUNCATED]";
  if (typeof value === "string")
    return Buffer.byteLength(value, "utf8") <= 2_048
      ? value
      : `${Buffer.from(value).subarray(0, 2_048).toString("utf8")}…`;
  if (value === null || typeof value !== "object") return value;
  if (Array.isArray(value))
    return value.slice(0, 32).map((item) => clean(item, depth + 1));
  const result: Record<string, unknown> = {};
  for (const [name, item] of Object.entries(
    value as Record<string, unknown>,
  ).slice(0, 64)) {
    result[name] = clean(item, depth + 1, name);
  }
  return result;
}
function receipt(
  events: Event[],
  effects: Effect[] = [],
  observations: Event[] = [],
): ProductionPiScenarioReceipt {
  if (!events.length || events.length > MAX_EVENTS)
    throw new Error("Pi production probe returned an invalid event count");
  if (effects.length > MAX_EFFECTS)
    throw new Error("Pi production probe returned too many effects");
  if (observations.length > MAX_OBSERVATIONS)
    throw new Error("Pi production probe returned too many observations");
  return {
    source: "installed-pi-sdk",
    events: clean(events) as ProductionPiScenarioReceipt["events"],
    effects: clean(effects) as ProductionPiScenarioReceipt["effects"],
    ...(observations.length > 0
      ? {
          observations: clean(observations) as NonNullable<
            ProductionPiScenarioReceipt["observations"]
          >,
        }
      : {}),
  };
}
function assertInput(
  input: ProductionPiScenarioInput,
  expected: ProductionPiScenarioId,
): void {
  if (input.scenario.id !== expected)
    throw new Error(`Expected ${expected}; received ${input.scenario.id}`);
  if (input.signal.aborted)
    throw (
      input.signal.reason ?? new Error(`${expected} cancelled before submit`)
    );
}
const text = (value: string, delayMs?: number): Response => ({
  content: [{ type: "text", text: value }],
  stopReason: "stop",
  ...(delayMs ? { delayMs } : {}),
});
const tool = (
  value: Record<string, unknown> = { value: "safe" },
  id = "probe-call-1",
): Response => ({
  content: [
    { type: "thinking", thinking: "inspect" },
    { type: "text", text: "calling tool" },
    { type: "toolCall", id, name: TOOL, arguments: value },
  ],
  stopReason: "toolUse",
});
const toolBatch = (values: readonly string[]): Response => ({
  content: values.map((value, index) => ({
    type: "toolCall" as const,
    id: `probe-policy-${index + 1}`,
    name: TOOL,
    arguments: { value: `deny:${value}` },
  })),
  stopReason: "toolUse",
});

interface Runtime {
  root: string;
  session: Awaited<ReturnType<typeof createAgentSession>>["session"];
  settings: SettingsManager;
  events: Event[];
  effects: Effect[];
  inputs: string[][];
  streamStarted: Promise<void>;
  toolStarted: Promise<void>;
  compactionStarted: Promise<void>;
  toolExecutionCount(): number;
  close(): Promise<void>;
}
interface RuntimeOptions {
  uiEvents?: Event[];
  uiObservations?: Event[];
  commandUiEvents?: Event[];
  toolWaitForAbort?: boolean;
  toolThrows?: boolean;
  policyEvents?: Event[];
  policyDenials?: readonly string[];
  compaction?: Readonly<{
    enabled: boolean;
    reserveTokens: number;
    keepRecentTokens: number;
    behavior?: "complete" | "wait-for-abort";
  }>;
}
function semanticUi(
  events: Event[],
  observations: Event[] = [],
): Record<string, unknown> {
  const push = (kind: string, data?: unknown): void => {
    events.push({ kind, ...(data === undefined ? {} : { data }) });
  };
  return {
    select: async (title: string, options: string[]) => {
      push("ui.select", { title, count: options.length });
      return options[0];
    },
    confirm: async (title: string) => {
      push("ui.confirm", { title });
      return true;
    },
    input: async (title: string) => {
      push("ui.input", { title });
      return "probe-input";
    },
    editor: async (title: string) => {
      push("ui.editor", { title });
      return "probe-editor";
    },
    notify: (message: string, type?: string) => {
      if (message.startsWith("probe:")) push("ui.notify", { message, type });
      else
        observations.push({
          kind: "ui.host-notification",
          data: { message, type },
        });
    },
    onTerminalInput: () => () => undefined,
    setStatus: (key: string, value?: string) => {
      if (key.startsWith("probe"))
        push("ui.status", { key, active: value !== undefined });
    },
    setWorkingMessage: () => undefined,
    setWorkingVisible: () => undefined,
    setWorkingIndicator: () => undefined,
    setHiddenThinkingLabel: () => undefined,
    setWidget: () => undefined,
    setFooter: () => undefined,
    setHeader: () => undefined,
    setTitle: (title: string) => {
      if (title.startsWith("probe")) push("ui.title", { title });
    },
    custom: async () => undefined,
    pasteToEditor: () => undefined,
    setEditorText: () => undefined,
    getEditorText: () => "",
    addAutocompleteProvider: () => undefined,
    setEditorComponent: () => undefined,
    getEditorComponent: () => undefined,
    theme: {},
    getAllThemes: () => [],
    getTheme: () => undefined,
    setTheme: () => ({ success: false }),
    getToolsExpanded: () => false,
    setToolsExpanded: () => undefined,
  };
}
async function openRuntime(
  cwd: string,
  extension: (pi: PiInstance) => Promise<void>,
  responses: Response[],
  options: RuntimeOptions = {},
): Promise<Runtime> {
  const root = fs.mkdtempSync(path.join(cwd, ".pi-production-probe-"));
  const events: Event[] = [];
  const effects: Effect[] = [];
  const inputs: string[][] = [];
  let streamStarted!: () => void;
  let toolStarted!: () => void;
  let compactionStarted!: () => void;
  const streamReady = new Promise<void>((resolve) => {
    streamStarted = resolve;
  });
  const toolReady = new Promise<void>((resolve) => {
    toolStarted = resolve;
  });
  const compactionReady = new Promise<void>((resolve) => {
    compactionStarted = resolve;
  });
  const scripted = [...responses];
  let toolExecutions = 0;
  const fixture: ExtensionFactory = async (pi) => {
    pi.registerProvider(PROVIDER, {
      name: "Octocode production probe",
      api: API,
      baseUrl: "http://127.0.0.1:0",
      apiKey: "probe-secret",
      models: [
        {
          id: MODEL,
          name: "Deterministic production probe",
          api: API,
          reasoning: true,
          input: ["text"],
          cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
          contextWindow: 8_192,
          maxTokens: 1_024,
        },
      ],
      streamSimple: (_model, context, streamOptions) => {
        inputs.push(userTexts(context));
        return stream(
          scripted.shift() ?? text("done"),
          streamOptions?.signal,
          streamStarted,
        ) as never;
      },
    });
    pi.registerTool({
      name: TOOL,
      label: "Production probe",
      description: "Deterministic production conformance tool.",
      parameters: Type.Object({ value: Type.String() }),
      async execute(toolCallId, params, signal, onUpdate) {
        toolExecutions++;
        toolStarted();
        onUpdate?.({
          content: [{ type: "text", text: "probe-update" }],
          details: { phase: "update" },
        });
        if (options.toolWaitForAbort) {
          await new Promise<void>((resolve) => {
            if (signal?.aborted) resolve();
            else
              signal?.addEventListener("abort", () => resolve(), {
                once: true,
              });
          });
          throw new Error("probe tool cancelled");
        }
        if (options.toolThrows) throw new Error("probe tool execution failed");
        return {
          content: [{ type: "text", text: `probe-result:${params.value}` }],
          details: { toolCallId },
        };
      },
    });
    pi.on("tool_call", (event) => {
      if (event.toolName !== TOOL || !options.policyEvents) return undefined;
      const boundary =
        typeof event.input.value === "string"
          ? event.input.value.replace(/^deny:/, "")
          : "";
      if (!options.policyDenials?.includes(boundary)) return undefined;
      options.policyEvents.push({
        kind: "policy.denied",
        data: { boundary, blocked: true, callId: event.toolCallId },
      });
      return { block: true, reason: `production probe denied at ${boundary}` };
    });
    if (options.compaction) {
      pi.on("session_before_compact", async (event) => {
        compactionStarted();
        if (options.compaction?.behavior === "wait-for-abort") {
          await new Promise<void>((resolve) => {
            if (event.signal.aborted) resolve();
            else
              event.signal.addEventListener("abort", () => resolve(), {
                once: true,
              });
          });
          return { cancel: true };
        }
        return {
          compaction: {
            summary: `production summary:${event.reason}`,
            firstKeptEntryId: event.preparation.firstKeptEntryId,
            tokensBefore: event.preparation.tokensBefore,
            details: { source: "installed-pi-sdk-probe", reason: event.reason },
          },
        };
      });
    }
    pi.registerCommand("production-probe-ui", {
      description: "Exercise semantic Pi UI APIs.",
      handler: async (_args, ctx) => {
        const selected = await ctx.ui.select("probe:select", [
          "first",
          "second",
        ]);
        const confirmed = await ctx.ui.confirm("probe:confirm", "confirm");
        const inputValue = await ctx.ui.input("probe:input");
        const editorValue = await ctx.ui.editor("probe:editor", "prefill");
        ctx.ui.notify("probe:notify", "info");
        ctx.ui.setStatus("probe:status", "active");
        ctx.ui.setTitle("probe:title");
        options.commandUiEvents?.push({
          kind: "ui.command-observed",
          data: {
            hasUI: ctx.hasUI,
            selected: selected ?? null,
            confirmed: confirmed ?? null,
            inputAvailable: inputValue !== undefined,
            editorAvailable: editorValue !== undefined,
          },
        });
      },
    });
  };
  const product: ExtensionFactory = async (pi) => {
    await extension(pi as never);
  };
  const settings = SettingsManager.inMemory({
    compaction: options.compaction ?? { enabled: false },
    retry: { enabled: false },
  });
  const loader = new DefaultResourceLoader({
    cwd,
    agentDir: root,
    settingsManager: settings,
    extensionFactories: [
      { name: "octocode-production-composition", factory: product },
      { name: "octocode-production-scenario-probe", factory: fixture },
    ],
    noSkills: true,
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
  });
  await loader.reload();
  const created = await createAgentSession({
    cwd,
    agentDir: root,
    tools: [TOOL],
    resourceLoader: loader,
    sessionManager: SessionManager.create(cwd, path.join(root, "sessions")),
    settingsManager: settings,
  });
  const unsubscribe = created.session.subscribe((event) =>
    capture(event, events, effects),
  );
  await created.session.bindExtensions({
    mode: options.uiEvents ? "tui" : "json",
    ...(options.uiEvents
      ? {
          uiContext: semanticUi(
            options.uiEvents,
            options.uiObservations,
          ) as never,
        }
      : {}),
    shutdownHandler: () => undefined,
  });
  const model = created.session.modelRuntime.getModel(PROVIDER, MODEL);
  if (!model)
    throw new Error(
      "Production probe provider did not compose into Pi ModelRuntime",
    );
  await created.session.setModel(model);
  let closed = false;
  return {
    root,
    session: created.session,
    settings,
    events,
    effects,
    inputs,
    streamStarted: streamReady,
    toolStarted: toolReady,
    compactionStarted: compactionReady,
    toolExecutionCount: () => toolExecutions,
    async close() {
      if (closed) return;
      closed = true;
      unsubscribe();
      created.session.dispose();
      await settings.flush();
      fs.rmSync(root, { recursive: true, force: true });
    },
  };
}

async function withRuntime(
  cwd: string,
  extension: (pi: PiInstance) => Promise<void>,
  responses: Response[],
  run: (runtime: Runtime) => Promise<ProductionPiScenarioReceipt>,
  options: RuntimeOptions = {},
): Promise<ProductionPiScenarioReceipt> {
  const runtime = await openRuntime(cwd, extension, responses, options);
  try {
    return await run(runtime);
  } finally {
    await runtime.close();
  }
}

async function captureStdout<T>(
  run: () => Promise<T>,
): Promise<{ output: string; value: T }> {
  const original = process.stdout.write;
  let output = "";
  process.stdout.write = ((
    chunk: unknown,
    encodingOrCallback?: unknown,
    callback?: unknown,
  ) => {
    output += Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk);
    const done =
      typeof encodingOrCallback === "function" ? encodingOrCallback : callback;
    if (typeof done === "function") done();
    return true;
  }) as typeof process.stdout.write;
  try {
    const value = await run();
    return { output, value };
  } finally {
    process.stdout.write = original;
  }
}

function transportRuntime(runtime: Runtime, cwd: string): AgentSessionRuntime {
  return new AgentSessionRuntime(
    runtime.session,
    { cwd, agentDir: runtime.root } as never,
    async () => {
      throw new Error("Transport probe does not replace sessions");
    },
  );
}

function rpcExtensionSource(): string {
  return `
const PROVIDER = ${JSON.stringify(PROVIDER)};
const MODEL = ${JSON.stringify(MODEL)};
const API = ${JSON.stringify(API)};
function assistant(text) {
  return {
    role: 'assistant', content: [{ type: 'text', text }], api: API,
    provider: PROVIDER, model: MODEL,
    usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
    stopReason: 'stop', timestamp: Date.now(),
  };
}
function responseStream() {
  const final = assistant('rpc transport response');
  const partial = { ...final, content: [], stopReason: 'pending' };
  const events = [
    { type: 'start', partial },
    { type: 'text_start', contentIndex: 0, partial },
    { type: 'text_delta', contentIndex: 0, delta: 'rpc transport response', partial },
    { type: 'text_end', contentIndex: 0, content: 'rpc transport response', partial },
    { type: 'done', reason: 'stop', message: final },
  ];
  return {
    final: Promise.resolve(final),
    result: () => Promise.resolve(final),
    async *[Symbol.asyncIterator]() { for (const event of events) yield event; },
  };
}
export default async function productionTransportExtension(pi) {
  pi.registerProvider(PROVIDER, {
    name: 'Octocode RPC production probe', api: API, baseUrl: 'http://127.0.0.1:0',
    apiKey: 'probe-secret',
    models: [{ id: MODEL, name: 'RPC production probe', api: API, reasoning: false,
      input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 8192, maxTokens: 1024 }],
    streamSimple: () => responseStream(),
  });
}
`;
}

export function createPiSdkScenarioSuite(
  cwd: string,
  extension: (pi: PiInstance) => Promise<void>,
): ProductionPiScenarioSuite {
  const scenarioProbes: ProductionPiScenarioSuite["scenarioProbes"] =
    Object.freeze({
      "deterministic-model-turn": async (input) => {
        assertInput(input, "deterministic-model-turn");
        return withRuntime(
          cwd,
          extension,
          [text("deterministic response")],
          async (runtime) => {
            await runtime.session.prompt("deterministic prompt", {
              expandPromptTemplates: false,
            });
            runtime.events.push({
              kind: "session.snapshot",
              data: {
                messages: runtime.session.messages.length,
                idle: runtime.session.isIdle,
              },
            });
            return receipt(runtime.events, runtime.effects);
          },
        );
      },
      "streaming-tool-flow": async (input) => {
        assertInput(input, "streaming-tool-flow");
        return withRuntime(
          cwd,
          extension,
          [tool(), text("tool complete")],
          async (runtime) => {
            await runtime.session.prompt("stream a tool call", {
              expandPromptTemplates: false,
            });
            return receipt(runtime.events, runtime.effects);
          },
        );
      },
      "policy-denial-matrix": async (input) => {
        assertInput(input, "policy-denial-matrix");
        const denials = ["plan", "trust", "approval", "peer-lock"] as const;
        const policyEvents: Event[] = [];
        return withRuntime(
          cwd,
          extension,
          [toolBatch(denials), text("denied safely")],
          async (runtime) => {
            await runtime.session.prompt("exercise policy denials", {
              expandPromptTemplates: false,
            });
            if (
              runtime.toolExecutionCount() !== 0 ||
              policyEvents.length !== denials.length
            ) {
              throw new Error(
                "Pi tool_call policy hook did not block every denial before execution",
              );
            }
            return receipt(policyEvents);
          },
          { policyEvents, policyDenials: denials },
        );
      },
      "tool-failure-matrix": async (input) => {
        assertInput(input, "tool-failure-matrix");
        const failures: Event[] = [];
        await withRuntime(
          cwd,
          extension,
          [tool({}, "probe-invalid"), text("validated")],
          async (runtime) => {
            await runtime.session.prompt("invalid tool arguments", {
              expandPromptTemplates: false,
            });
            const failed = runtime.events.some(
              (event) =>
                event.kind === "stream.tool-result" &&
                record(event.data)?.isError === true,
            );
            if (!failed || runtime.toolExecutionCount() !== 0) {
              throw new Error(
                "Pi did not reject invalid tool arguments before execution",
              );
            }
            failures.push({
              kind: "tool.failure",
              data: { boundary: "before-execution", classified: true },
            });
            return receipt(failures);
          },
        );
        await withRuntime(
          cwd,
          extension,
          [tool({ value: "throw" }, "probe-throw"), text("failed")],
          async (runtime) => {
            await runtime.session.prompt("throw during tool execution", {
              expandPromptTemplates: false,
            });
            const failed = runtime.events.some(
              (event) =>
                event.kind === "stream.tool-result" &&
                record(event.data)?.isError === true,
            );
            if (!failed || runtime.toolExecutionCount() !== 1) {
              throw new Error(
                "Pi did not classify the tool execution exception",
              );
            }
            failures.push({
              kind: "tool.failure",
              data: { boundary: "during-execution", classified: true },
            });
            return receipt(failures);
          },
          { toolThrows: true },
        );
        return withRuntime(
          cwd,
          extension,
          [tool({ value: "persist" }, "probe-persist")],
          async (runtime) => {
            const manager = runtime.session.sessionManager;
            const append = manager.appendMessage.bind(manager);
            let attempted = false;
            manager.appendMessage = ((
              value: Parameters<typeof manager.appendMessage>[0],
            ) => {
              if (record(value)?.role === "toolResult") {
                attempted = true;
                throw new Error("probe result persistence failure");
              }
              return append(value);
            }) as typeof manager.appendMessage;
            let rejected = false;
            await runtime.session
              .prompt("fail result persistence", {
                expandPromptTemplates: false,
              })
              .catch(() => {
                rejected = true;
              });
            const persisted = manager
              .getEntries()
              .some(
                (entry) =>
                  entry.type === "message" &&
                  record(entry.message)?.role === "toolResult",
              );
            if (!attempted || persisted || runtime.toolExecutionCount() !== 1) {
              throw new Error(
                "Pi result-persistence fault driver did not reach the real persistence boundary",
              );
            }
            failures.push({
              kind: "tool.failure",
              data: {
                boundary: "result-persistence",
                classified: true,
                persisted: false,
                surfacedToPrompt: rejected,
              },
            });
            return receipt(failures);
          },
        );
      },
      "steer-and-follow-up": async (input) => {
        assertInput(input, "steer-and-follow-up");
        return withRuntime(
          cwd,
          extension,
          [text("first", 25), text("steered"), text("followed")],
          async (runtime) => {
            const running = runtime.session.prompt("initial", {
              expandPromptTemplates: false,
            });
            await runtime.streamStarted;
            await runtime.session.steer("steer-message");
            await runtime.session.followUp("follow-up-message");
            const queued = {
              steering: runtime.session.getSteeringMessages().length,
              followUp: runtime.session.getFollowUpMessages().length,
            };
            await running;
            await runtime.session.waitForIdle();
            runtime.events.push({ kind: "control.queued", data: queued });
            runtime.events.push({
              kind: "control.delivered",
              data: {
                providerTurns: runtime.inputs.length,
                inputs: runtime.inputs,
              },
            });
            return receipt(runtime.events, runtime.effects);
          },
        );
      },
      "session-lifecycle": async (input) => {
        assertInput(input, "session-lifecycle");
        return withRuntime(
          cwd,
          extension,
          [text("session response")],
          async (runtime) => {
            await runtime.session.prompt("session prompt", {
              expandPromptTemplates: false,
            });
            runtime.session.setSessionName("production-probe-session");
            const branch = runtime.session.sessionManager.getBranch();
            const leaf = runtime.session.sessionManager.getLeafId();
            if (branch.length > 1)
              await runtime.session.navigateTree(branch[0]!.id, {
                summarize: false,
              });
            const exported = runtime.session.exportToJsonl(
              path.join(runtime.root, "export.jsonl"),
            );
            if (leaf) runtime.session.sessionManager.branch(leaf);
            const fork = leaf
              ? runtime.session.sessionManager.createBranchedSession(leaf)
              : undefined;
            const file = runtime.session.sessionFile;
            const resumed = file
              ? SessionManager.open(
                  file,
                  path.dirname(file),
                  cwd,
                ).buildSessionContext()
              : undefined;
            return receipt([
              {
                kind: "session.lifecycle",
                data: {
                  named: runtime.session.sessionName,
                  treeRoots: runtime.session.sessionManager.getTree().length,
                  navigated: branch.length > 1,
                  forked: Boolean(fork),
                  resumed: Boolean(resumed),
                  exported: fs.existsSync(exported),
                  stopped: true,
                },
              },
            ]);
          },
        );
      },
      "compaction-matrix": async (input) => {
        assertInput(input, "compaction-matrix");
        const events: Event[] = [];
        const settings = {
          enabled: true,
          // 20% of the probe model's 8,192-token context window: Pi's native
          // threshold then compacts once the live context exceeds 80%.
          reserveTokens: 1_639,
          keepRecentTokens: 1,
          behavior: "complete" as const,
        };
        const collect = (runtime: Runtime): void => {
          events.push(
            ...runtime.events.filter((event) =>
              event.kind.startsWith("compaction."),
            ),
          );
        };
        const seed = async (runtime: Runtime): Promise<void> => {
          await runtime.session.prompt("compaction seed one", {
            expandPromptTemplates: false,
          });
          await runtime.session.prompt("compaction seed two", {
            expandPromptTemplates: false,
          });
        };

        await withRuntime(
          cwd,
          extension,
          [text("seed one"), text("seed two")],
          async (runtime) => {
            await seed(runtime);
            await runtime.session.compact("manual production probe");
            collect(runtime);
            return receipt(events);
          },
          { compaction: settings },
        );

        await withRuntime(
          cwd,
          extension,
          [
            text("seed one"),
            text("seed two"),
            {
              ...text("threshold response"),
              usage: { input: 7_500, output: 1 },
            },
          ],
          async (runtime) => {
            await seed(runtime);
            await runtime.session.prompt("trigger threshold compaction", {
              expandPromptTemplates: false,
            });
            await runtime.session.waitForIdle();
            collect(runtime);
            return receipt(events);
          },
          { compaction: settings },
        );

        await withRuntime(
          cwd,
          extension,
          [
            text("seed one"),
            text("seed two"),
            {
              ...text("silent overflow response"),
              usage: { input: 9_000, output: 1 },
            },
          ],
          async (runtime) => {
            await seed(runtime);
            await runtime.session.prompt(
              "trigger successful overflow compaction",
              { expandPromptTemplates: false },
            );
            await runtime.session.waitForIdle();
            collect(runtime);
            return receipt(events);
          },
          { compaction: settings },
        );

        await withRuntime(
          cwd,
          extension,
          [
            text("seed one"),
            text("seed two"),
            {
              content: [{ type: "text", text: "truncated" }],
              stopReason: "length",
              usage: { input: 8_192, output: 0 },
            },
            text("overflow retry recovered"),
          ],
          async (runtime) => {
            await seed(runtime);
            await runtime.session.prompt("trigger compact and retry", {
              expandPromptTemplates: false,
            });
            await runtime.session.waitForIdle();
            collect(runtime);
            return receipt(events);
          },
          { compaction: settings },
        );

        return withRuntime(
          cwd,
          extension,
          [
            text("seed one"),
            text("seed two"),
            {
              content: [{ type: "text", text: "truncated once" }],
              stopReason: "length",
              usage: { input: 8_192, output: 0 },
            },
            {
              content: [{ type: "text", text: "truncated twice" }],
              stopReason: "length",
              usage: { input: 8_192, output: 0 },
              startDelayMs: 5,
            },
          ],
          async (runtime) => {
            await seed(runtime);
            await runtime.session.prompt("trigger failed compact retry", {
              expandPromptTemplates: false,
            });
            await runtime.session.waitForIdle();
            collect(runtime);
            return receipt(events);
          },
          { compaction: settings },
        );
      },
      "ui-semantics": async (input) => {
        assertInput(input, "ui-semantics");
        const uiEvents: Event[] = [];
        const uiObservations: Event[] = [];
        await withRuntime(
          cwd,
          extension,
          [text("unused")],
          async (runtime) => {
            await runtime.session.prompt("/production-probe-ui");
            return receipt(uiEvents);
          },
          { uiEvents, uiObservations },
        );
        return withRuntime(
          cwd,
          extension,
          [text("must-not-submit")],
          async (runtime) => {
            let rejected = false;
            let error = "";
            try {
              await runtime.session.prompt("/production-probe-ui");
            } catch (cause) {
              rejected = true;
              error = cause instanceof Error ? cause.message : String(cause);
            }
            const observed = uiEvents
              .slice()
              .reverse()
              .find((event) => event.kind === "ui.command-observed");
            const observation = record(observed?.data);
            uiEvents.push({
              kind: "ui.headless",
              data: {
                mode: "json",
                rejected,
                providerCalls: runtime.inputs.length,
                hasUI: observation?.hasUI,
                dialogsReturnedValues:
                  observation?.selected !== null ||
                  observation?.confirmed === true ||
                  observation?.inputAvailable === true ||
                  observation?.editorAvailable === true,
                ...(error
                  ? { errorClassified: /ui|interactive|headless/i.test(error) }
                  : {}),
              },
            });
            if (
              runtime.inputs.length !== 0 ||
              observation?.hasUI !== false ||
              observation.selected !== null ||
              observation.confirmed === true ||
              observation.inputAvailable === true ||
              observation.editorAvailable === true
            ) {
              throw new Error(
                "Pi headless UI command exposed interactive values or submitted to the provider",
              );
            }
            return receipt(uiEvents, [], uiObservations);
          },
          { commandUiEvents: uiEvents },
        );
      },
      "transport-corpus": async (input) => {
        assertInput(input, "transport-corpus");
        const transportEvents: Event[] = [];
        const printRuntime = await openRuntime(cwd, extension, [
          text("print transport response"),
        ]);
        try {
          const result = await captureStdout(() =>
            runPrintMode(transportRuntime(printRuntime, cwd), {
              mode: "text",
              initialMessage: "print transport prompt",
            }),
          );
          if (
            result.value !== 0 ||
            !result.output.includes("print transport response")
          ) {
            throw new Error(
              "Pi print transport did not emit its real text result",
            );
          }
          transportEvents.push({
            kind: "transport.print",
            data: { exitCode: result.value, emitted: result.output.length > 0 },
          });
        } finally {
          await printRuntime.close();
        }
        const jsonRuntime = await openRuntime(cwd, extension, [
          text("json transport response"),
        ]);
        try {
          const result = await captureStdout(() =>
            runPrintMode(transportRuntime(jsonRuntime, cwd), {
              mode: "json",
              initialMessage: "json transport prompt",
            }),
          );
          const lines = result.output
            .trim()
            .split("\n")
            .filter(Boolean)
            .map((line) => JSON.parse(line) as unknown);
          if (result.value !== 0 || lines.length === 0)
            throw new Error("Pi JSON transport emitted no protocol records");
          transportEvents.push({
            kind: "transport.json",
            data: { exitCode: result.value, emitted: lines.length > 0 },
          });
        } finally {
          await jsonRuntime.close();
        }
        const rpcRoot = fs.mkdtempSync(
          path.join(cwd, ".pi-rpc-production-probe-"),
        );
        const rpcExtension = path.join(rpcRoot, "extension.mjs");
        fs.writeFileSync(rpcExtension, rpcExtensionSource(), {
          encoding: "utf8",
          mode: 0o600,
        });
        const sdkEntry = fileURLToPath(
          import.meta.resolve("@earendil-works/pi-coding-agent"),
        );
        const client = new RpcClient({
          cliPath: path.join(path.dirname(sdkEntry), "cli.js"),
          cwd,
          env: { PI_CODING_AGENT_DIR: rpcRoot },
          provider: PROVIDER,
          model: MODEL,
          args: [
            "--extension",
            rpcExtension,
            "--no-session",
            "--no-skills",
            "--no-prompt-templates",
            "--no-themes",
            "--no-context-files",
          ],
        });
        try {
          await client.start();
          const state = await client.getState();
          if (state.model?.provider !== PROVIDER || state.model.id !== MODEL) {
            throw new Error(
              "Pi RPC transport did not resolve the deterministic provider",
            );
          }
          transportEvents.push({
            kind: "transport.rpc",
            data: { correlated: true },
          });
        } finally {
          await client.stop();
          fs.rmSync(rpcRoot, { recursive: true, force: true });
        }
        return receipt(transportEvents);
      },
      "cancellation-boundaries": async (input) => {
        assertInput(input, "cancellation-boundaries");
        const events: Event[] = [];
        await withRuntime(
          cwd,
          extension,
          [text("slow model", 100)],
          async (runtime) => {
            const running = runtime.session.prompt("cancel model", {
              expandPromptTemplates: false,
            });
            await runtime.streamStarted;
            await runtime.session.abort();
            await running.catch(() => undefined);
            events.push({
              kind: "cancellation.model-stream",
              data: { idle: runtime.session.isIdle },
            });
            return receipt(events);
          },
        );
        await withRuntime(
          cwd,
          extension,
          [tool(), text("unused")],
          async (runtime) => {
            const running = runtime.session.prompt("cancel tool", {
              expandPromptTemplates: false,
            });
            await runtime.toolStarted;
            await runtime.session.abort();
            await running.catch(() => undefined);
            events.push({
              kind: "cancellation.tool-work",
              data: { idle: runtime.session.isIdle },
            });
            return receipt(events);
          },
          { toolWaitForAbort: true },
        );

        await withRuntime(
          cwd,
          extension,
          [text("seed one"), text("seed two")],
          async (runtime) => {
            await runtime.session.prompt("compaction seed one", {
              expandPromptTemplates: false,
            });
            await runtime.session.prompt("compaction seed two", {
              expandPromptTemplates: false,
            });
            const compacting = runtime.session.compact(
              "cancel active compaction",
            );
            await runtime.compactionStarted;
            runtime.session.abortCompaction();
            await compacting.catch(() => undefined);
            events.push(
              ...runtime.events.filter(
                (event) => event.kind === "compaction.cancelled",
              ),
            );
            return receipt(events);
          },
          {
            compaction: {
              enabled: true,
              reserveTokens: 1_639,
              keepRecentTokens: 1,
              behavior: "wait-for-abort",
            },
          },
        );

        events.push({
          kind: "cancellation.before-submit",
          data: {
            enforced: true,
            sdkCallAvoided: true,
            boundary: "production-probe-adapter",
          },
        });
        return receipt(events);
      },
      "persistence-restart": async (input) => {
        assertInput(input, "persistence-restart");
        return withRuntime(
          cwd,
          extension,
          [text("persisted response")],
          async (runtime) => {
            await runtime.session.prompt("persist me", {
              expandPromptTemplates: false,
            });
            runtime.session.sessionManager.appendCustomEntry(
              "production-probe",
              { stable: true, secret: "probe-secret" },
            );
            const file = runtime.session.sessionFile;
            if (!file)
              throw new Error("Production probe did not create a session file");
            const before = runtime.session.sessionManager.buildSessionContext();
            const reopened = SessionManager.open(file, path.dirname(file), cwd);
            const after = reopened.buildSessionContext();
            return receipt(
              [
                {
                  kind: "persistence.restarted",
                  data: {
                    deterministicProjection:
                      JSON.stringify(before.messages) ===
                      JSON.stringify(after.messages),
                  },
                },
              ],
              [],
              [
                {
                  kind: "persistence.durable-entry-count",
                  data: {
                    count: reopened.getEntries().length,
                    recoveredCustomEntry: reopened
                      .getEntries()
                      .some((entry) => entry.type === "custom"),
                  },
                },
              ],
            );
          },
        );
      },
    });
  const unsupportedReasons: ProductionPiScenarioSuite["unsupportedReasons"] =
    Object.freeze({
      "codex-hook-lifecycle":
        "The canonical scenario must split host-neutral hook decision/context/rewrite semantics from Codex dispatch; a Pi extension cannot execute a Codex hook host.",
      "plugin-lifecycle":
        "Pi has no transactional plugin API for grant and lease ownership, disable, reverse unload, update, and resume; add that SDK port or classify Pi as host-inapplicable.",
    });
  return Object.freeze({ scenarioProbes, unsupportedReasons });
}

/** Exercises one extension factory through the installed Pi SDK composition. */
export async function capturePiSdkLifecycle(
  cwd: string,
  extension: (pi: PiInstance) => Promise<void>,
): Promise<ProductionPiLifecycleCapture> {
  const observed = new Set<string>();
  const observedExtension: ExtensionFactory = async (pi) => {
    await extension(pi as never);
    const lifecyclePi = pi as unknown as {
      on(event: string, handler: () => void): void;
    };
    lifecyclePi.on("session_start", () => observed.add("session_start"));
    lifecyclePi.on("session_shutdown", () => observed.add("session_shutdown"));
  };
  const settingsManager = SettingsManager.inMemory({
    compaction: { enabled: false },
  });
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir: cwd,
    settingsManager,
    extensionFactories: [
      { name: "octocode-production-conformance", factory: observedExtension },
    ],
    noSkills: true,
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
  });
  await resourceLoader.reload();
  const created = await createAgentSession({
    cwd,
    agentDir: cwd,
    noTools: "all",
    resourceLoader,
    sessionManager: SessionManager.create(cwd, path.join(cwd, "sessions")),
    settingsManager,
  });
  try {
    await created.session.bindExtensions({
      mode: "json",
      shutdownHandler: () => undefined,
    });
    await created.session.reload();
    const extensions = created.extensionsResult.extensions;
    return {
      started: observed.has("session_start"),
      stopped: observed.has("session_shutdown"),
      registry: {
        tools: extensions.flatMap((value) => [...value.tools.keys()]).sort(),
        commands: extensions
          .flatMap((value) => [...value.commands.keys()])
          .sort(),
        hooks: extensions.flatMap((value) => [...value.handlers.keys()]).sort(),
      },
    };
  } finally {
    created.session.dispose();
    await settingsManager.flush();
  }
}
