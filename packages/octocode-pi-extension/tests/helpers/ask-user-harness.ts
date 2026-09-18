import { registerAskUserTool } from '../../src/tools/ask-user-tool.js';
import { configureInteractionBrokerRoute } from '../../src/tools/interaction-broker.js';
import type { PiContext, ToolDefinition } from '../../src/types.js';

export function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (d: ToolDefinition) => tools.set(d.name, d) };
  registerAskUserTool(pi, new Set<string>(), (p, names, def) => {
    names.add(def.name);
    p.registerTool?.(def);
  });
  const tool = tools.get('askUser')!;
  const execute = tool.execute.bind(tool);
  tool.execute = (id, params, signal, onUpdate, ctx) => {
    const envelope = Array.isArray(params['queries'])
      ? params
      : { queries: [{ reasoning: 'resolve a genuine test decision', ...params }] };
    return execute(id, envelope, signal, onUpdate, ctx);
  };
  return tool;
}

// Inline harness: askUser renders via ctx.ui.custom(builder) with NO overlay
// options, so it appears inline in the message flow. The mock invokes the
// factory synchronously, captures the component + any opts (expected undefined),
// and resolves the custom() promise when the factory calls done().
export function overlayCtx(terminalRows?: number) {
  let component: { render(w: number): string[]; handleInput(d: string): void } | undefined;
  let overlayOpts: { overlay?: boolean } | undefined;
  const pendingInputs: string[] = [];
  const tui = { requestRender: () => {}, terminal: { rows: terminalRows } };
  const ctx = {
    hasUI: true,
    mode: 'tui',
    ui: {
      custom: (
        factory: (tui: unknown, theme: unknown, kb: unknown, done: (v: unknown) => void) => { render(w: number): string[]; handleInput(d: string): void },
        opts?: { overlay?: boolean },
      ) =>
        new Promise((resolve) => {
          overlayOpts = opts;
          component = factory(tui, undefined, undefined, (v) => resolve(v));
          for (const input of pendingInputs.splice(0)) component.handleInput(input);
        }),
    },
  } as unknown as PiContext;
  configureInteractionBrokerRoute(ctx, true);
  return {
    ctx,
    resize: (rows: number) => { tui.terminal.rows = rows; },
    send: (data: string) => {
      if (component) component.handleInput(data);
      else pendingInputs.push(data);
    },
    render: (w = 100) => component?.render(w) ?? [],
    overlayOpts: () => overlayOpts,
    // Simulate the TUI granting focus (Focusable.focused = true).
    focus: () => { if (component) (component as { focused?: boolean }).focused = true; },
  };
}
