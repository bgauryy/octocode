import type { ExtensionContext, Theme } from '@earendil-works/pi-coding-agent';
import { truncateToWidth, visibleWidth, type Component } from '@earendil-works/pi-tui';
import { formatAgo, formatCost, formatDuration, formatTokenUsage, toolCallCount } from '../shared/format.js';
import { clip, plural, statusColor } from '../shared/render.js';
import { STATUS } from '../shared/status.js';
import type { Team } from './session.js';
import { GOAL_SHOWN_CHARS } from './identity.js';
import type { Member, Traffic } from './model.js';

/** The panel widget's key. The widget is static text only: its rows are pure in (members, width, now), so repainting never moves the transcript. */
const WIDGET_KEY = 'octocode-agents';
const MAX_ROWS = 6;
/** Recent messages shown under the agents, picked after filtering out other sessions' traffic. */
const TRAFFIC_ROWS = 3;

/** Rows come from other processes' database rows: untrusted text, sanitized and cut to one line before drawing. */
const safe = (text: string | undefined, max = 200) => (text ? clip(text, max) : '');

/** An agent whose heartbeat (every 5 s) stopped this long ago is killed or hung: it leaves the panel. */
const UNRESPONSIVE_MS = 15_000;
const MODEL_MAX = 28;

type Cell = { text: string; color: Parameters<Theme['fg']>[0]; right?: boolean };
/**
 * Optional columns, dropped left to right when the terminal is too narrow (flags first). Name, goal, model, state, elapsed
 * time and tokens always stay; when they do not fit either, the goal is shortened.
 */
const OPTIONAL = ['flags', 'tools', 'cost'] as const;
type Column = 'glyph' | 'name' | 'goal' | 'model' | 'state' | 'age' | 'tokens' | (typeof OPTIONAL)[number];
const ORDER: Column[] = ['glyph', 'name', 'goal', 'model', 'state', 'age', 'tokens', 'tools', 'cost', 'flags'];
const SEPARATOR = ' · ';
/** The goal is shortened down to this many columns before anything else on the row is cut. */
const GOAL_MIN = 12;
/** Columns kept for the current activity before optional columns are dropped. */
const ACTIVITY_MIN = 20;

/** `anthropic/claude-opus-4-5` → `claude-opus-4-5`: the provider adds width, not meaning, in a one-line row. */
const modelLabel = (model: string | undefined) => safe(model?.slice(model.lastIndexOf('/') + 1), MODEL_MAX);


/**
 * One agent's cells. The row says who it is (name), what it is for (goal), which model it runs, its state, how long it
 * has run, its tokens (with the cached share), and what it does right now; its tools, estimated cost and flags when the
 * width allows.
 */
function cells(member: Member, depth: number, now: number): Record<Column, Cell> & { detail: string } {
  const working = member.status === 'working';
  const detail = working ? safe(member.activity) : '';
  const flags = [member.pending ? `✉ ${member.pending} waiting` : '', member.locks?.length ? plural(member.locks.length, 'lock') : ''].filter(Boolean).join('  ');
  return {
    glyph: { text: working ? '●' : '○', color: working ? 'accent' : 'dim' },
    name: { text: `${depth > 0 ? `${'  '.repeat(depth - 1)} └ ` : ''}${safe(member.id, 64)}`, color: working ? 'text' : 'muted' },
    goal: { text: safe(member.goal, GOAL_SHOWN_CHARS), color: working ? 'text' : 'muted' },
    model: { text: modelLabel(member.model), color: 'muted' },
    state: { text: working ? 'working' : 'idle', color: working ? 'accent' : 'dim' },
    age: { text: formatDuration(now - member.joinedAt), color: 'dim', right: true },
    tools: { text: toolCallCount(member.toolCalls), color: 'dim', right: true },
    tokens: { text: formatTokenUsage(member), color: 'dim', right: true },
    cost: { text: formatCost(member), color: 'dim', right: true },
    flags: { text: flags, color: member.pending ? 'warning' : 'dim' },
    detail,
  };
}

/** `text` cut to at most `width` terminal columns, ending in `…` when cut (plain text: no escape codes added). */
function fitWidth(text: string, width: number): string {
  if (visibleWidth(text) <= width) return text;
  let chars = width;
  let cut = clip(text, chars);
  while (chars > 1 && visibleWidth(cut) > width) cut = clip(text, --chars);
  return cut;
}

/** Rows with every column padded to its widest cell, so state, time and stats line up down the list. */
function memberRows(tree: Array<{ member: Member; depth: number }>, now: number, theme: Theme, width: number): string[] {
  const rows = tree.map(({ member, depth }) => cells(member, depth, now));
  const goalWidth = Math.max(...rows.map((row) => visibleWidth(row.goal.text)));
  let goalCap = goalWidth;
  const widthOf = (column: Column) => (column === 'goal' ? Math.min(goalWidth, goalCap) : Math.max(...rows.map((row) => visibleWidth(row[column].text))));
  let columns = ORDER.filter((column) => widthOf(column) > 0);
  const used = () => 1 + columns.reduce((sum, column) => sum + widthOf(column) + (column === 'glyph' ? 1 : SEPARATOR.length), 0);
  // Keep room for the activity: drop optional columns until it fits.
  for (const column of OPTIONAL) if (used() + ACTIVITY_MIN > width) columns = columns.filter((kept) => kept !== column);
  // Still too wide for the columns that always stay: shorten the goal rather than cut tokens, time or model.
  if (used() > width) goalCap = Math.max(GOAL_MIN, goalWidth - (used() - width));
  for (const row of rows) row.goal.text = fitWidth(row.goal.text, goalCap);
  return rows.map((row) => {
    // Trailing empty cells (no cost yet, nothing waiting) leave no dangling separator.
    const last = columns.reduce((found, column, index) => (row[column].text !== '' ? index : found), -1);
    const parts = columns.slice(0, last + 1).map((column, index) => {
      const cell = row[column];
      const pad = ' '.repeat(widthOf(column) - visibleWidth(cell.text));
      const text = theme.fg(cell.color, cell.text);
      return (cell.right ? pad + text : text + pad) + (index === last ? '' : column === 'glyph' ? ' ' : theme.fg('dim', SEPARATOR));
    });
    const detail = row.detail ? `  ${theme.fg('muted', row.detail)}` : '';
    return truncateToWidth(` ${parts.join('')}${detail}`.trimEnd(), Math.max(1, width), '…');
  });
}

const STATE_COLOR = { queued: 'dim', delivered: 'muted', 'awaiting reply': 'warning', answered: 'success', 'dead-lettered': 'error' } as const;

/** `✉ #12 main-a → researcher-b · awaiting reply  "check the tests"` */
export function trafficRow(message: Traffic, now: number, theme: Theme, width: number): string {
  const to = safe(message.to.length > 2 ? `${message.to[0]} +${message.to.length - 1}` : message.to.join(', '), 80) || 'nobody';
  const head = `${theme.fg('accent', '✉')} ${theme.fg('dim', `#${message.id}${message.replyTo ? `↩#${message.replyTo}` : ''}`)} ${theme.fg('text', safe(message.from, 64))} ${theme.fg('dim', '→')} ${theme.fg('text', to)}`;
  const state = theme.fg(STATE_COLOR[message.state], message.state);
  const body = theme.fg('muted', `"${safe(message.text)}"`);
  return truncateToWidth(` ${head} ${theme.fg('dim', `${formatAgo(now - message.at)} ·`)} ${state}  ${body}`, Math.max(1, width), '…');
}

/**
 * This session's own subagents, transitively (children, grandchildren, ...). The team database is shared by every
 * Pi process in the repository, so other sessions and their subagents are left out of this session's panel.
 */
function descendants(members: Member[], selfId: string | undefined): Member[] {
  if (!selfId) return [];
  const mine = new Set([selfId]);
  const result: Member[] = [];
  let grew = true;
  while (grew) {
    grew = false;
    for (const member of members) {
      if (member.parentId && mine.has(member.parentId) && !mine.has(member.id)) {
        mine.add(member.id);
        result.push(member);
        grew = true;
      }
    }
  }
  return result;
}

/**
 * Messages between this session's subagents. Messages to or from this session are left out: they are already in its
 * transcript.
 */
function ownTraffic(traffic: Traffic[], members: Member[], selfId: string | undefined): Traffic[] {
  if (!selfId) return [];
  const mine = new Set(members.map((member) => member.id));
  return traffic.filter((message) => message.from !== selfId && !message.to.includes(selfId) && (mine.has(message.from) || message.to.some((to) => mine.has(to))));
}

/** Parents before their children, working agents first among siblings, each with its depth under this session. */
function treeOrder(members: Member[], selfId: string): Array<{ member: Member; depth: number }> {
  const out: Array<{ member: Member; depth: number }> = [];
  const visit = (parent: string, depth: number) => {
    const children = members.filter((member) => member.parentId === parent).sort((a, b) => Number(b.status === 'working') - Number(a.status === 'working') || a.joinedAt - b.joinedAt);
    for (const member of children) {
      out.push({ member, depth });
      visit(member.id, depth + 1);
    }
  };
  visit(selfId, 0);
  return out;
}

/** A killed or hung agent stops its heartbeat: leave it out (the store drops it for good once its process is gone). */
function liveMembers(members: Member[], now: number): Member[] {
  return members.filter((member) => now - member.updatedAt <= UNRESPONSIVE_MS);
}

/** Rows for the widget under the editor; empty when this session has no live subagents. Recent messages follow the agents. */
export function widgetLines(members: Member[], selfId: string | undefined, now: number, theme: Theme, width: number, traffic: Traffic[] = []): string[] {
  const others = descendants(liveMembers(members, now), selfId);
  if (others.length === 0 || !selfId) return [];
  const tree = treeOrder(others, selfId);
  const shown = tree.slice(0, MAX_ROWS);
  const busy = others.filter((member) => member.status === 'working').length;
  const cost = formatCost({ cost: others.reduce((total, member) => total + member.cost, 0), unpriced: others.some((member) => member.unpriced) });
  const idle = others.length - busy;
  const summary = [...(busy ? [`${busy} working`] : []), ...(idle ? [`${idle} idle`] : []), ...(cost ? [cost] : [])].join(' · ');
  const head = truncateToWidth(` ${theme.fg('toolTitle', theme.bold('agents'))}  ${theme.fg('muted', summary)}`, Math.max(1, width), '…');
  const more = tree.length - shown.length;
  return [head, ...memberRows(shown, now, theme, width), ...(more > 0 ? [truncateToWidth(theme.fg('muted', ` +${more} more`), Math.max(1, width), '…')] : []), ...ownTraffic(traffic, others, selfId).slice(0, TRAFFIC_ROWS).map((message) => trafficRow(message, now, theme, width))];
}

/**
 * Keeps the below-editor widget in step with the team while there is something to show; the footer segment only says
 * when the team is off (the widget already counts the agents). It reads nothing itself: the team session's ticker refreshes a cached view and calls back once a second.
 */
export class AgentsView {
  private unwatch: (() => void) | undefined;
  private members: Member[] = [];
  private traffic: Traffic[] = [];
  private shown = false;
  /** Last footer text sent: an unchanged one is not sent again on every tick. */
  private status: string | undefined;
  private render: (() => void) | undefined;
  /** Background subagents this session runs, and the footer text last sent for them. */
  private background = 0;
  private backgroundStatus: string | undefined;
  private ctx: ExtensionContext | undefined;

  /** Reads only the team's id, its cached view and the once-a-second watch. */
  constructor(private readonly team: Pick<Team, 'id' | 'view' | 'watch'>) {}

  start(ctx: ExtensionContext): void {
    if (!ctx.hasUI) return;
    this.stop(ctx);
    this.ctx = ctx;
    this.showBackground(ctx);
    // A terminal widget: RPC clients ignore component factories, so the 1 s ticker would feed nothing there.
    if (ctx.mode !== 'tui') return;
    this.unwatch = this.team.watch(() => this.paint(ctx));
  }

  /** How many background subagents run now: counted in the footer while the panel does not show them. */
  setBackground(count: number): void {
    this.background = count;
    if (this.ctx) this.showBackground(this.ctx);
  }

  private showBackground(ctx: ExtensionContext): void {
    const text = this.background > 0 && !this.shown ? statusColor(ctx, 'accent', plural(this.background, 'background agent')) : undefined;
    if (text === this.backgroundStatus) return;
    this.backgroundStatus = text;
    try {
      ctx.ui.setStatus(STATUS.subagents, text);
    } catch {
      // The session was replaced.
    }
  }

  stop(ctx?: ExtensionContext): void {
    this.unwatch?.();
    this.unwatch = undefined;
    this.ctx = undefined;
    if (ctx && this.backgroundStatus !== undefined) {
      this.backgroundStatus = undefined;
      try {
        ctx.ui.setStatus(STATUS.subagents, undefined);
      } catch {
        // The session was replaced.
      }
    }
    if (ctx && (this.shown || this.status !== undefined)) this.clear(ctx);
    this.render = undefined;
    this.members = [];
    this.traffic = [];
  }

  private setStatus(ctx: ExtensionContext, text: string | undefined): void {
    if (text === this.status) return;
    this.status = text;
    ctx.ui.setStatus(STATUS.team, text);
  }

  private clear(ctx: ExtensionContext): void {
    this.shown = false;
    this.status = undefined;
    try {
      ctx.ui.setWidget(WIDGET_KEY, undefined);
      ctx.ui.setStatus(STATUS.team, undefined);
    } catch {
      // The session was replaced.
    }
  }

  private paint(ctx: ExtensionContext): void {
    try {
      const { members, traffic, error, retrying } = this.team.view;
      this.members = members;
      this.traffic = traffic;
      const self = this.team.id;
      // The same liveness filter as widgetLines: a widget with no rows is never mounted.
      const visible = descendants(liveMembers(this.members, Date.now()), self).length > 0;
      if (error) {
        // A foreign database turns the team off for this session; a locked one is retried on the next tick.
        // Either way the widget keeps its state, and mounts once a read succeeds.
        this.setStatus(ctx, retrying ? statusColor(ctx, 'warning', `team retrying: ${safe(error, 80)}`) : statusColor(ctx, 'error', `team off: ${safe(error, 80)}`));
        this.showBackground(ctx);
        return;
      }
      this.setStatus(ctx, undefined);
      if (!visible) {
        if (this.shown) this.clear(ctx);
        this.showBackground(ctx);
        return;
      }
      if (!this.shown) {
        this.shown = true;
        ctx.ui.setWidget(
          WIDGET_KEY,
          (tui, theme): Component => {
            this.render = () => tui.requestRender();
            return {
              render: (width: number) => widgetLines(this.members, this.team.id, Date.now(), theme, width, this.traffic),
              invalidate: () => undefined,
            };
          },
          { placement: 'belowEditor' },
        );
      }
      this.render?.();
      this.showBackground(ctx);
    } catch {
      // The session was replaced while painting.
    }
  }
}
