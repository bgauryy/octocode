import fs from 'node:fs';
import os from 'node:os';
import type { OctocodeMcp, OctocodeOwner } from '../src/mcp/octocode.js';
import type { ExtensionAPI, ExtensionCommandContext, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Permission } from '../src/permission/ask.js';
import type { PermissionMode } from '../src/permission/mode.js';

type EditorFactory = ReturnType<ExtensionContext['ui']['getEditorComponent']>;

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
  const shortcuts = new Map<string, { description?: string; handler: (ctx: unknown) => unknown }>();
  const sent: Array<{ message: any; options: any }> = [];
  const entries: Array<{ customType: string; data: unknown }> = [];
  const labels = new Map<string, string>();
  let activeTools: string[] | undefined;
  const pi = {
    getSettings: () => ({}),
    registerTool: (tool: { name: string }) => tools.set(tool.name, tool),
    registerCommand: (name: string, command: unknown) => commands.set(name, command),
    registerMessageRenderer: (type: string, renderer: unknown) => renderers.set(type, renderer),
    registerEntryRenderer: (type: string, renderer: unknown) => renderers.set(type, renderer),
    registerFlag: () => undefined,
    registerShortcut: (key: string, options: { description?: string; handler: (ctx: unknown) => unknown }) => shortcuts.set(key, options),
    getFlag: () => undefined,
    on: (event: string, handler: Handler) => {
      const list = handlers.get(event) ?? [];
      list.push(handler);
      handlers.set(event, list);
    },
    sendMessage: (message: unknown, options: unknown) => sent.push({ message, options }),
    sendUserMessage: (message: unknown, options: unknown) => sent.push({ message, options }),
    appendEntry: (customType: string, data: unknown) => entries.push({ customType, data }),
    setLabel: (id: string, label: string | undefined) => (label === undefined ? labels.delete(id) : labels.set(id, label)),
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
  return { pi: pi as unknown as ExtensionAPI, tools, commands, handlers, renderers, shortcuts, sent, entries, labels, emit, fire };
}

interface FakeUi {
  notes: Array<{ message: string; type?: string }>;
  statuses: Map<string, string | undefined>;
  widgets: Map<string, unknown>;
  footer?: unknown;
  header?: unknown;
  editor?: EditorFactory | undefined;
  title?: string;
  selects: Array<string | undefined>;
  inputs: Array<string | undefined>;
  confirms: boolean[];
  custom?: (factory: any) => Promise<unknown>;
}

/** A ctx whose ui records notifications, statuses and widgets and answers dialogs from queued replies. */
export function fakeCtx(options: { cwd: string; hasUI?: boolean; mode?: string; idle?: boolean; ui?: Partial<FakeUi>; model?: unknown }): ExtensionCommandContext & { ui: FakeUi & ExtensionContext['ui'] } {
  const state: FakeUi = { notes: [], statuses: new Map(), widgets: new Map(), selects: [], inputs: [], confirms: [], ...options.ui };
  const ui = {
    ...state,
    notify: (message: string, type?: string) => state.notes.push({ message, type }),
    setStatus: (key: string, text: string | undefined) => state.statuses.set(key, text),
    setWidget: (key: string, content: unknown) => (content === undefined ? state.widgets.delete(key) : state.widgets.set(key, content)),
    setFooter: (factory: unknown) => ((ui as FakeUi).footer = factory),
    setHeader: (factory: unknown) => ((ui as FakeUi).header = factory),
    setEditorComponent: (factory: EditorFactory | undefined) => ((ui as FakeUi).editor = factory),
    getEditorComponent: () => (ui as FakeUi).editor,
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
    sessionManager: { getSessionId: () => 'session-test', getEntries: () => [], getBranch: () => [], getSessionFile: () => undefined, getHeader: () => null, getCwd: () => options.cwd, getSessionName: () => undefined, buildSessionProjection: () => ({ entries: [], messages: [] }) },
    model: options.model,
    getContextUsage: () => undefined,
  } as never;
}

/** A stand-in for the Octocode server a turn asks about: who runs it, whether it connects, and its instructions. */
export function fakeOctocode(owner: OctocodeOwner = { by: 'extension' }) {
  const state = { owner, connected: true, instructions: undefined as string | undefined, readyCalls: 0 };
  const mcp: Pick<OctocodeMcp, 'owner' | 'ready' | 'instructions'> = {
    owner: () => state.owner,
    ready: async () => {
      state.readyCalls++;
      return state.connected;
    },
    get instructions() {
      return state.instructions;
    },
  };
  return { mcp, state };
}

/** A started permission state in `mode` (default workspace: in-repo work runs, anything else asks). */
export function fakePermission(mode: PermissionMode = 'workspace', options: { pi?: ExtensionAPI; isSubagent?: boolean; stderr?: (text: string) => void; snapshotMs?: number } = {}): Permission {
  const permission = new Permission({ pi: options.pi ?? fakePi().pi, env: { OCTOCODE_PERMISSION: mode }, isSubagent: options.isSubagent ?? false, stderr: options.stderr ?? (() => undefined), ...(options.snapshotMs ? { snapshotMs: options.snapshotMs } : {}) });
  // The OS temp folder: tests/setup-env.ts saved a trust decision for it.
  permission.start({ cwd: fs.realpathSync(os.tmpdir()), isProjectTrusted: () => true, hasUI: false, sessionManager: { getEntries: () => [], getSessionFile: () => undefined } } as never);
  return permission;
}
