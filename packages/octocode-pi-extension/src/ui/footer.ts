import os from 'node:os';
import path from 'node:path';
import type { ExtensionAPI, ExtensionContext, Theme } from '@earendil-works/pi-coding-agent';
import { truncateToWidth, visibleWidth } from '@earendil-works/pi-tui';
import { formatTokens } from '../shared/format.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';

/**
 * Octocode's two-line footer, in the order other coding agents converged on: line 1 is what the agent is and is doing
 * (model, thinking, context, review mode, running agents and bash jobs; speed and cost on the right), line 2 is where
 * it works (cwd, branch, session name, tokens, other extensions' statuses such as Pi's MCP and Octocode's hooks). Pure in (state, statuses, width) and
 * never animated: Pi repaints the footer on every streamed token.
 */

export interface FooterState {
  model?: string | undefined;
  /** Thinking level, only for models that reason. */
  thinking?: string | undefined;
  /** `percent` is null right after compaction, until the next response. */
  context?: { percent: number | null; window: number } | undefined;
  usage: { input: number; output: number; cost: number };
  cwd: string;
  home?: string | undefined;
  sessionName?: string | undefined;
}

type Color = Parameters<Theme['fg']>[0];
type Paint = Pick<Theme, 'fg' | 'bold'>;

/** Status keys this footer places itself; any other extension's status is appended to line 2. */
const OWN = { team: 'octocode-team', bash: 'octocode-bash', review: 'octocode-review', speed: 'octocode-speed' } as const;
const BAR_CELLS = 8;
const BRANCH_MAX = 24;

/** An 8-cell gauge and its colour: green under half, yellow to 80%, red above. */
export function contextBar(percent: number): { bar: string; color: Color } {
  const filled = Math.min(BAR_CELLS, Math.max(0, Math.floor((percent / 100) * BAR_CELLS)));
  return { bar: '▰'.repeat(filled) + '▱'.repeat(BAR_CELLS - filled), color: percent > 80 ? 'error' : percent >= 50 ? 'warning' : 'success' };
}

/**
 * A part of a line. Parts drop (or switch to `short`) lowest `priority` first until the line fits; `keep` parts are
 * never dropped, only shortened.
 */
interface Part {
  text: string;
  priority: number;
  keep?: boolean;
  short?: string;
}

function tilde(cwd: string, home: string | undefined): string {
  if (!home) return cwd;
  const relative = path.relative(home, cwd);
  return relative === '' ? '~' : relative.startsWith('..') || path.isAbsolute(relative) ? cwd : `~${path.sep}${relative}`;
}

/** Joins the left parts with dim dots and pushes the right parts to the edge, shortening and dropping until it fits. */
function fit(left: Part[], right: Part[], theme: Paint, width: number): string {
  const dot = theme.fg('dim', ' · ');
  let parts = [...left, ...right].filter((part) => part.text !== '');
  const render = () => {
    const l = left.filter((part) => parts.includes(part)).map((part) => part.text).join(dot);
    const r = right.filter((part) => parts.includes(part)).map((part) => part.text).join(dot);
    if (!r) return l;
    return `${l}${' '.repeat(Math.max(2, width - visibleWidth(l) - visibleWidth(r)))}${r}`;
  };
  while (visibleWidth(render()) > width) {
    const next = parts.filter((part) => !part.keep || part.short !== undefined).sort((a, b) => a.priority - b.priority)[0];
    if (!next) break;
    if (next.short !== undefined) {
      next.text = next.short;
      delete next.short;
    } else parts = parts.filter((part) => part !== next);
  }
  return truncateToWidth(render(), Math.max(1, width), theme.fg('dim', '…'));
}

export function footerLines(state: FooterState, branch: string | null, statuses: ReadonlyMap<string, string>, theme: Paint, width: number): string[] {
  const status = (key: string) => statuses.get(key) ?? '';
  const context = state.context;
  let ctx: Part = { text: '', priority: 60, keep: true };
  if (context) {
    const window = formatTokens(context.window);
    if (context.percent === null) ctx = { ...ctx, text: `${theme.fg('muted', 'ctx')} ${theme.fg('dim', `? of ${window}`)}` };
    else {
      const { bar, color } = contextBar(context.percent);
      const percent = theme.fg(color, `${Math.round(context.percent)}%`);
      ctx = { ...ctx, text: `${theme.fg('muted', 'ctx')} ${theme.fg(color, bar)} ${percent} ${theme.fg('dim', `of ${window}`)}`, short: `${theme.fg('muted', 'ctx')} ${percent}` };
    }
  }
  const top = fit(
    [
      { text: theme.fg('accent', state.model ?? 'no model'), priority: 100, keep: true },
      { text: state.thinking ? theme.fg('muted', state.thinking) : '', priority: 30 },
      ctx,
      { text: status(OWN.review), priority: 90, keep: true },
      { text: status(OWN.team), priority: 90, keep: true },
      { text: status(OWN.bash), priority: 90, keep: true },
    ],
    [
      { text: status(OWN.speed) ? theme.fg('dim', status(OWN.speed)) : '', priority: 10 },
      { text: state.usage.cost > 0 ? theme.fg('muted', `$${state.usage.cost.toFixed(2)}`) : '', priority: 40 },
    ],
    theme,
    width,
  );

  const place = tilde(state.cwd, state.home);
  const shownBranch = branch && branch.length > BRANCH_MAX ? `${branch.slice(0, BRANCH_MAX - 1)}…` : branch;
  const where = (dir: string) => `${theme.fg('text', dir)}${shownBranch ? ` ${theme.fg('muted', `⎇ ${shownBranch}`)}` : ''}`;
  const tokens = state.usage.input || state.usage.output ? theme.fg('dim', `↑${formatTokens(state.usage.input)} ↓${formatTokens(state.usage.output)}`) : '';
  const others = [...statuses].filter(([key]) => !(Object.values(OWN) as string[]).includes(key)).sort(([a], [b]) => a.localeCompare(b));
  const bottom = fit(
    [
      { text: where(place), priority: 100, keep: true, short: where(path.basename(state.cwd)) },
      // Session names come from anywhere (RPC, other extensions); Pi strips only newlines.
      { text: state.sessionName ? theme.fg('muted', sanitizeTerminalText(state.sessionName)) : '', priority: 50 },
      { text: tokens, priority: 60 },
      ...others.map(([, text]) => ({ text, priority: 20 })),
    ],
    [],
    theme,
    width,
  );
  return [top, bottom];
}

type Usage = { input?: number; output?: number; cost?: { total?: number } } | undefined;

/** The usage a session entry adds to the totals: assistant replies and tool results that carry one (subagents). */
function entryUsage(entry: unknown): Usage {
  const message = (entry as { type?: string; message?: { role?: string; usage?: Usage } }).message;
  if ((entry as { type?: string }).type !== 'message' || !message) return undefined;
  return message.role === 'assistant' || message.role === 'toolResult' ? message.usage : undefined;
}

/**
 * Replaces Pi's footer with `footerLines`. The state is sampled on events (session start, each finished message,
 * model and thinking changes, compaction), never inside render: context usage walks the whole session.
 */
export function registerFooter(pi: ExtensionAPI): void {
  const state: FooterState = { usage: { input: 0, output: 0, cost: 0 }, cwd: process.cwd(), home: os.homedir() };
  let repaint: (() => void) | undefined;
  const add = (usage: Usage) => {
    state.usage.input += usage?.input ?? 0;
    state.usage.output += usage?.output ?? 0;
    state.usage.cost += usage?.cost?.total ?? 0;
  };
  const sample = (ctx: ExtensionContext) => {
    const model = ctx.model;
    state.model = model?.id;
    state.thinking = model?.reasoning ? pi.getThinkingLevel() : undefined;
    const usage = ctx.getContextUsage();
    state.context = usage ? { percent: usage.percent, window: usage.contextWindow } : model ? { percent: null, window: model.contextWindow } : undefined;
    state.cwd = ctx.cwd;
    state.sessionName = ctx.sessionManager.getSessionName?.();
    repaint?.();
  };
  // The footer is a terminal component (setFooter does nothing over RPC), so its bookkeeping runs only in the TUI.
  const refresh = async (_event: unknown, ctx: ExtensionContext) => {
    if (ctx.hasUI && ctx.mode === 'tui') sample(ctx);
  };

  pi.on('session_start', async (_event, ctx) => {
    if (!ctx.hasUI || ctx.mode !== 'tui') return;
    state.usage = { input: 0, output: 0, cost: 0 };
    for (const entry of ctx.sessionManager.getEntries()) add(entryUsage(entry));
    sample(ctx);
    ctx.ui.setFooter((tui, theme, data) => {
      repaint = () => tui.requestRender();
      const unsubscribe = data.onBranchChange(() => tui.requestRender());
      return {
        render: (width: number) => footerLines(state, data.getGitBranch(), data.getExtensionStatuses(), theme, width),
        invalidate: () => undefined,
        dispose: () => {
          unsubscribe();
          repaint = undefined;
        },
      };
    });
  });
  pi.on('message_end', async (event, ctx) => {
    if (!ctx.hasUI || ctx.mode !== 'tui') return;
    add(entryUsage({ type: 'message', message: event.message }));
    sample(ctx);
  });
  pi.on('model_select', refresh);
  pi.on('thinking_level_select', refresh);
  pi.on('session_compact', refresh);
  pi.on('agent_end', refresh);
  pi.on('session_info_changed', refresh);
}
