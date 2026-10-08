import type { ExtensionContext, Theme } from '@earendil-works/pi-coding-agent';
import { truncateToWidth, visibleWidth, type Component } from '@earendil-works/pi-tui';
import { formatDuration } from '../shared/format.js';
import { clip, statusColor } from '../shared/render.js';
import type { Team } from './session.js';
import type { Member, Traffic } from './model.js';

/** Static text only: the rows are pure in (members, width, now), so repainting never moves the transcript. */

const WIDGET_KEY = 'octocode-agents';
const STATUS_KEY = 'octocode-team';
const MAX_ROWS = 6;
/** Recent messages shown under the agents, picked after filtering out other sessions' traffic. */
const TRAFFIC_ROWS = 3;

/** Rows come from other processes' database rows: untrusted text, sanitized and cut to one line before drawing. */
const safe = (text: string | undefined, max = 200) => (text ? clip(text, max) : '');

/** An agent whose heartbeat (every 5 s) stopped this long ago is killed or hung: it leaves the panel. */
const UNRESPONSIVE_MS = 15_000;
const MODEL_MAX = 28;

type Cell = { text: string; color: Parameters<Theme['fg']>[0]; right?: boolean };
/** Optional columns, dropped left to right when the terminal is too narrow; name, state and activity always stay. */
const OPTIONAL = ['cost', 'tools', 'flags', 'age', 'model'] as const;
type Column = 'glyph' | 'name' | 'state' | (typeof OPTIONAL)[number];
const ORDER: Column[] = ['glyph', 'name', 'model', 'state', 'age', 'tools', 'cost', 'flags'];

/** `anthropic/claude-opus-4-5` → `claude-opus-4-5`: the provider adds width, not meaning, in a one-line row. */
export const modelLabel = (model: string | undefined) => safe(model?.slice(model.lastIndexOf('/') + 1), MODEL_MAX);

const plural = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;

/**
 * One agent's cells. The row says who it is, which model it runs, its state, and what it does right now; the task it
 * was given (a long prompt) stays in `coordinate list`, and raw token counters (cumulative re-sent context) stay out.
 */
function cells(member: Member, depth: number, now: number): Record<Column, Cell> & { detail: string } {
  const working = member.status === 'working';
  const detail = working ? safe(member.activity) : '';
  const flags = [member.pending ? `✉ ${member.pending} waiting` : '', member.locks?.length ? plural(member.locks.length, 'lock') : ''].filter(Boolean).join('  ');
  return {
    glyph: { text: working ? '●' : '○', color: working ? 'accent' : 'dim' },
    name: { text: `${depth > 0 ? `${'  '.repeat(depth - 1)} └ ` : ''}${safe(member.id, 64)}`, color: working ? 'text' : 'muted' },
    model: { text: modelLabel(member.model), color: 'muted' },
    state: { text: working ? 'working' : 'idle', color: working ? 'accent' : 'dim' },
    age: { text: formatDuration(now - member.joinedAt), color: 'dim', right: true },
    tools: { text: plural(member.toolCalls, 'tool'), color: 'dim', right: true },
    cost: { text: member.cost > 0 ? `$${member.cost.toFixed(2)}` : '', color: 'dim', right: true },
    flags: { text: flags, color: member.pending ? 'warning' : 'dim' },
    detail,
  };
}

/** Rows with every column padded to its widest cell, so state, time and stats line up down the list. */
function memberRows(tree: Array<{ member: Member; depth: number }>, now: number, theme: Theme, width: number): string[] {
  const rows = tree.map(({ member, depth }) => cells(member, depth, now));
  const widthOf = (column: Column) => Math.max(...rows.map((row) => visibleWidth(row[column].text)));
  let columns = ORDER.filter((column) => widthOf(column) > 0);
  // Keep at least 20 columns for the activity: drop optional columns until it fits.
  const used = () => 1 + columns.reduce((sum, column) => sum + widthOf(column) + (column === 'glyph' ? 1 : 2), 0);
  for (const column of OPTIONAL) if (used() + 20 > width) columns = columns.filter((kept) => kept !== column);
  return rows.map((row) => {
    const parts = columns.map((column, index) => {
      const cell = row[column];
      const pad = ' '.repeat(widthOf(column) - visibleWidth(cell.text));
      const text = theme.fg(cell.color, cell.text);
      return (cell.right ? pad + text : text + pad) + (index === columns.length - 1 ? '' : column === 'glyph' ? ' ' : '  ');
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
  return truncateToWidth(` ${head} ${theme.fg('dim', `${formatDuration(now - message.at)} ago ·`)} ${state}  ${body}`, Math.max(1, width), '…');
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
  const cost = others.reduce((total, member) => total + member.cost, 0);
  const idle = others.length - busy;
  const summary = [...(busy ? [`${busy} working`] : []), ...(idle ? [`${idle} idle`] : []), ...(cost > 0 ? [`$${cost.toFixed(2)}`] : [])].join(' · ');
  const head = truncateToWidth(` ${theme.fg('toolTitle', theme.bold('agents'))}  ${theme.fg('muted', summary)}`, Math.max(1, width), '…');
  const more = tree.length - shown.length;
  return [head, ...memberRows(shown, now, theme, width), ...(more > 0 ? [theme.fg('muted', ` +${more} more`)] : []), ...ownTraffic(traffic, others, selfId).slice(0, TRAFFIC_ROWS).map((message) => trafficRow(message, now, theme, width))];
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

  constructor(private readonly team: Team) {}

  start(ctx: ExtensionContext): void {
    // A terminal widget: RPC clients ignore component factories, so the 1 s ticker would feed nothing there.
    if (!ctx.hasUI || ctx.mode !== 'tui') return;
    this.stop(ctx);
    this.unwatch = this.team.watch(() => this.paint(ctx));
  }

  stop(ctx?: ExtensionContext): void {
    this.unwatch?.();
    this.unwatch = undefined;
    if (ctx && this.shown) this.clear(ctx);
    this.render = undefined;
    this.members = [];
    this.traffic = [];
  }

  private setStatus(ctx: ExtensionContext, text: string | undefined): void {
    if (text === this.status) return;
    this.status = text;
    ctx.ui.setStatus(STATUS_KEY, text);
  }

  private clear(ctx: ExtensionContext): void {
    this.shown = false;
    this.status = undefined;
    try {
      ctx.ui.setWidget(WIDGET_KEY, undefined);
      ctx.ui.setStatus(STATUS_KEY, undefined);
    } catch {
      // The session was replaced.
    }
  }

  private paint(ctx: ExtensionContext): void {
    try {
      const { members, traffic, error } = this.team.view;
      this.members = members;
      this.traffic = traffic;
      const self = this.team.id;
      // The same liveness filter as widgetLines: a widget with no rows is never mounted.
      const visible = descendants(liveMembers(this.members, Date.now()), self).length > 0;
      if (error) {
        // A foreign or newer database: the team is off for this session, and the footer says why.
        this.shown = true;
        this.setStatus(ctx, statusColor(ctx, 'error', `team off: ${safe(error, 80)}`));
        return;
      }
      this.setStatus(ctx, undefined);
      if (!visible) {
        if (this.shown) this.clear(ctx);
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
    } catch {
      // The session was replaced while painting.
    }
  }
}
