import type { ExtensionAPI, ExtensionCommandContext, ExtensionContext } from '@earendil-works/pi-coding-agent';

/** Identity theme: colours and styles pass text through, so rendered components read as plain text. */
export const theme = {
  fg: (_color: string, text: string) => text,
  bg: (_color: string, text: string) => text,
  bold: (text: string) => text,
  italic: (text: string) => text,
  strikethrough: (text: string) => text,
  underline: (text: string) => text,
} as never;

/** The text a rendered component shows at `width`, lines trimmed at the end. */
export function rendered(component: { render(width: number): string[] }, width = 200): string {
  return component.render(width).map((line) => line.trimEnd()).join('\n');
}

type Handler = (event: unknown, ctx: unknown) => unknown;

/** Records every registration a Pi extension makes and lets a test fire its events. */
export function fakePi() {
  const tools = new Map<string, any>();
  const commands = new Map<string, any>();
  const handlers = new Map<string, Handler[]>();
  const renderers = new Map<string, any>();
  const sent: Array<{ message: any; options: any }> = [];
  const entries: Array<{ customType: string; data: unknown }> = [];
  const mcpServers = new Map<string, any>();
  let activeTools: string[] | undefined;
  const pi = {
    registerMcpServer: (name: string, config: unknown) => mcpServers.set(name, config),
    getSettings: () => ({}),
    registerTool: (tool: { name: string }) => tools.set(tool.name, tool),
    registerCommand: (name: string, command: unknown) => commands.set(name, command),
    registerMessageRenderer: (type: string, renderer: unknown) => renderers.set(type, renderer),
    registerEntryRenderer: (type: string, renderer: unknown) => renderers.set(type, renderer),
    registerFlag: () => undefined,
    registerShortcut: () => undefined,
    getFlag: () => undefined,
    on: (event: string, handler: Handler) => {
      const list = handlers.get(event) ?? [];
      list.push(handler);
      handlers.set(event, list);
    },
    sendMessage: (message: unknown, options: unknown) => sent.push({ message, options }),
    sendUserMessage: (message: unknown, options: unknown) => sent.push({ message, options }),
    appendEntry: (customType: string, data: unknown) => entries.push({ customType, data }),
    getActiveTools: () => activeTools ?? [...tools.keys()],
    getAllTools: () => [...tools.values()],
    setActiveTools: (names: string[]) => {
      activeTools = [...names];
    },
    getCommands: () => [],
    events: { on: () => () => undefined, emit: () => undefined },
  };
  const emit = async (event: string, payload: unknown, ctx: unknown) => {
    const results: unknown[] = [];
    for (const handler of handlers.get(event) ?? []) results.push(await handler(payload, ctx));
    return results;
  };
  /** Runs every handler for `event` in order and returns the last one's result, as Pi does for most events. */
  const fire = async (event: string, payload: unknown, ctx: unknown) => (await emit(event, payload, ctx)).at(-1);
  return { pi: pi as unknown as ExtensionAPI, tools, commands, handlers, renderers, sent, entries, mcpServers, emit, fire };
}

export interface FakeUi {
  notes: Array<{ message: string; type?: string }>;
  statuses: Map<string, string | undefined>;
  widgets: Map<string, unknown>;
  footer?: unknown;
  header?: unknown;
  title?: string;
  selects: Array<string | undefined>;
  inputs: Array<string | undefined>;
  confirms: boolean[];
  custom?: (factory: any) => Promise<unknown>;
}

/** A ctx whose ui records notifications, statuses and widgets and answers dialogs from queued replies. */
export function fakeCtx(options: { cwd: string; hasUI?: boolean; mode?: string; idle?: boolean; ui?: Partial<FakeUi> }): ExtensionCommandContext & { ui: FakeUi & ExtensionContext['ui'] } {
  const state: FakeUi = { notes: [], statuses: new Map(), widgets: new Map(), selects: [], inputs: [], confirms: [], ...options.ui };
  const ui = {
    ...state,
    notify: (message: string, type?: string) => state.notes.push({ message, type }),
    setStatus: (key: string, text: string | undefined) => state.statuses.set(key, text),
    setWidget: (key: string, content: unknown) => (content === undefined ? state.widgets.delete(key) : state.widgets.set(key, content)),
    setFooter: (factory: unknown) => ((ui as FakeUi).footer = factory),
    setHeader: (factory: unknown) => ((ui as FakeUi).header = factory),
    setTitle: (title: string) => ((ui as FakeUi).title = title),
    setWorkingMessage: (message?: string) => ((ui as FakeUi & { working?: string }).working = message),
    setWorkingIndicator: (indicator: unknown) => ((ui as FakeUi & { indicator?: unknown }).indicator = indicator),
    setHiddenThinkingLabel: (label?: string) => ((ui as FakeUi & { thinkingLabel?: string }).thinkingLabel = label),
    select: async () => state.selects.shift(),
    input: async () => state.inputs.shift(),
    confirm: async () => state.confirms.shift() ?? false,
    custom: (factory: unknown) => (state.custom ? state.custom(factory) : Promise.resolve(undefined)),
    theme,
  };
  // Share the recording arrays with the returned ui so tests can read them.
  Object.assign(ui, { notes: state.notes, statuses: state.statuses, widgets: state.widgets, selects: state.selects, inputs: state.inputs, confirms: state.confirms });
  return {
    cwd: options.cwd,
    hasUI: options.hasUI ?? true,
    mode: options.mode ?? 'tui',
    ui,
    isIdle: () => options.idle ?? true,
    isProjectTrusted: () => true,
    abort: () => undefined,
    hasPendingMessages: () => false,
    signal: undefined,
    sessionManager: { getSessionId: () => 'session-test', getEntries: () => [], getBranch: () => [], getSessionFile: () => undefined },
    model: undefined,
    getContextUsage: () => undefined,
  } as never;
}
