import { formatAgo, formatSpan } from '../shared/format.js';
import { visibleControls } from '../shared/sanitize.js';
import { refText } from './row.js';
import { priorityName, STATES, type Item, type Note, type State } from './item.js';
import type { BacklogStore } from './store.js';

export const STATE_TITLE: Record<State, string> = { ongoing: 'Ongoing', todo: 'Todo', backlog: 'Backlog (triage)', done: 'Done' };
/** Board and export order: the flow an item follows, so the board's ←/→ step up/down the sections as drawn. */
export const BOARD_ORDER: readonly State[] = STATES;

/** Who holds an item, from the reader's side: `(you)`, `(parent)` (a subagent's parent), `(@main-1a2b)`, or nothing. */
function holder(item: Item, self?: string, parents?: ReadonlySet<string>): string {
  if (!item.assignee || item.state !== 'ongoing') return '';
  if (item.assignee === self) return ' (you)';
  return parents?.has(item.assignee) ? ' (parent)' : ` (@${visibleControls(item.assignee)})`;
}

/** ` (from B12)` when the item came from another one. */
const fromText = (item: Item): string => (item.from === undefined ? '' : ` (from ${refText(item.from)})`);

/** `B14 [ongoing p1] Title #tag (from B12) (you)` */
export function itemLine(item: Item, self?: string, parents?: ReadonlySet<string>): string {
  const tags = item.tags.map((tag) => ` #${tag}`).join('');
  return `${item.ref} [${item.state} ${priorityName(item.priority)}] ${visibleControls(item.title)}${tags}${fromText(item)}${holder(item, self, parents)}`;
}

const ago = (at: number, now: number) => formatAgo(now - at);

/** Local wall-clock time, `2026-10-12 21:03`. */
function stamp(at: number): string {
  const date = new Date(at);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Who wrote a note or holds an item, from the reader's side: `you`, `your parent`, or the id. */
const who = (id: string, self?: string, parents?: ReadonlySet<string>) => (id === self ? 'you' : parents?.has(id) ? 'your parent' : visibleControls(id));

/**
 * When things happened to an item, oldest first: `created 2026-10-09 14:00 (3d ago) by agent · claimed 2h ago by you ·
 * done 5m ago (took 1h55m) · updated 5m ago`.
 */
function timeline(item: Item, now: number, self?: string, parents?: ReadonlySet<string>): string[] {
  const heldBy = item.assignee ? ` by ${who(item.assignee, self, parents)}` : '';
  const parts = [`created ${stamp(item.createdAt)} (${ago(item.createdAt, now)}) by ${visibleControls(item.createdBy)}`];
  if (item.startedAt) parts.push(`claimed ${ago(item.startedAt, now)}${item.state === 'done' ? '' : heldBy}`);
  else if (item.assignee && item.state !== 'done') parts.push(`assignee ${who(item.assignee, self, parents)}`);
  if (item.doneAt) parts.push(`done ${ago(item.doneAt, now)}${item.startedAt && item.startedAt <= item.doneAt ? ` (took ${formatSpan(item.doneAt - item.startedAt)})` : ''}${heldBy}`);
  if (item.updatedAt !== item.createdAt) parts.push(`updated ${ago(item.updatedAt, now)}`);
  return parts;
}

/** A note's text with later lines indented under its bullet (a change note may carry a whole replaced body). */
const indented = (text: string, indent: string) => visibleControls(text).replace(/\n/g, `\n${indent}`);

/** `- 5m ago you: text` for a note, `- 5m ago you · state todo → ongoing` for a change the store logged. */
function noteLine(note: Note, now: number, self?: string, parents?: ReadonlySet<string>): string {
  return `- ${ago(note.at, now)} ${who(note.author, self, parents)}${note.kind === 'change' ? ' ·' : ':'} ${indented(note.text, '  ')}`;
}

/** The item's saved fields in reading order, empty ones omitted: Goal, Where, Why, Evidence (one `-` line per entry), Plan. */
export function itemFields(item: Item): string[] {
  const lines: string[] = [];
  if (item.goal) lines.push(`Goal: ${visibleControls(item.goal)}`);
  if (item.paths.length) lines.push(`Where: ${item.paths.map((path) => visibleControls(path)).join(', ')}`);
  if (item.why) lines.push(`Why: ${indented(item.why, '  ')}`);
  if (item.evidence.length) lines.push('Evidence:', ...item.evidence.map((entry) => `- ${visibleControls(entry)}`));
  if (item.plan) lines.push(`Plan: ${indented(item.plan, '  ')}`);
  return lines;
}

interface DetailOptions {
  noteOffset?: number;
  self?: string;
  /** A subagent's parent sessions, shown as `(parent)`. */
  parents?: ReadonlySet<string>;
  /** Show the version (the board uses it for stale-edit checks; the tool has no use for it). */
  version?: boolean;
  /** The item this one came from, and the items that came from it (all of them). */
  origin?: Item;
  derived?: readonly Item[];
}

/** The whole item: header, goal, where, why, evidence, plan, times, origin, body, derived items and its notes. */
export function itemDetail(item: Item, notes: Note[], totalNotes: number, now: number, options: DetailOptions = {}): string {
  const { self, parents } = options;
  const meta = [...timeline(item, now, self, parents), options.version ? `version ${item.version}` : ''].filter(Boolean);
  const lines = [itemLine(item, self, parents), ...itemFields(item), meta.join(' · ')];
  if (options.origin) lines.push(`From: ${itemLine(options.origin, self, parents)}`);
  if (item.body) lines.push('', visibleControls(item.body));
  if (options.derived?.length) lines.push('', `Derived items (${options.derived.length}):`, ...options.derived.map((child) => `- ${itemLine(child, self, parents)}`));
  if (notes.length > 0) {
    const offset = options.noteOffset ?? 0;
    const label = offset > 0 ? `Notes and changes (${offset + 1}–${offset + notes.length} newest of ${totalNotes}):` : totalNotes > notes.length ? `Notes and changes (last ${notes.length} of ${totalNotes}):` : 'Notes and changes:';
    lines.push('', label, ...notes.map((note) => noteLine(note, now, self, parents)));
  }
  return lines.join('\n');
}

/** Every item grouped by state, for a headless `/octocode backlog`. */
export function boardText(store: BacklogStore, self?: string): string {
  const { items } = store.list();
  if (items.length === 0) return 'The backlog is empty. Add an item: /backlog add <title>';
  const sections = BOARD_ORDER.flatMap((state) => {
    const rows = items.filter((item) => item.state === state);
    return rows.length === 0 ? [] : [`${STATE_TITLE[state]} (${rows.length})`, ...rows.map((item) => `  ${itemLine(item, self)}`)];
  });
  return sections.join('\n');
}

/** Footer counts: `backlog 1 ongoing · 4 todo · 2 to triage · /backlog`; empty when there is nothing open. */
export function countsText(counts: Record<State, number>): string {
  const parts = [counts.ongoing ? `${counts.ongoing} ongoing` : '', counts.todo ? `${counts.todo} todo` : '', counts.backlog ? `${counts.backlog} to triage` : ''].filter(Boolean);
  return parts.length ? `backlog ${parts.join(' · ')} · /backlog` : '';
}

/** Markdown snapshot of the whole board with every note, for `<repo>/.octocode/backlog.md`. Nothing reads it back. */
export function exportMarkdown(store: BacklogStore, repoName: string, now: number): string {
  const { items } = store.list();
  const lines = [
    `<!-- Generated by Octocode (/backlog export) at ${new Date(now).toISOString()}. A snapshot: edits here are not read back. -->`,
    `# Backlog — ${visibleControls(repoName)}`,
    '',
  ];
  if (items.length === 0) lines.push('_Empty._', '');
  for (const state of BOARD_ORDER) {
    const rows = items.filter((item) => item.state === state);
    if (rows.length === 0) continue;
    lines.push(`## ${STATE_TITLE[state]} (${rows.length})`, '');
    for (const item of rows) {
      const tags = item.tags.map((tag) => ` #${tag}`).join('');
      const owner = item.assignee ? ` — ${visibleControls(item.assignee)}` : '';
      lines.push(`- **${item.ref}** [${priorityName(item.priority)}] ${visibleControls(item.title)}${tags}${fromText(item)}${owner}`);
      lines.push(...itemFields(item).flatMap((field) => field.split('\n')).map((line) => `  ${line}`));
      const times = [`created ${stamp(item.createdAt)} by ${visibleControls(item.createdBy)}`, item.startedAt ? `claimed ${stamp(item.startedAt)}` : '', item.doneAt ? `done ${stamp(item.doneAt)}` : ''];
      lines.push(`  _${times.filter(Boolean).join(' · ')}_`);
      for (const line of item.body ? visibleControls(item.body).split('\n') : []) lines.push(`  ${line}`.trimEnd());
      for (const note of store.notes(item, -1)) {
        const heading = `${new Date(note.at).toISOString().slice(0, 16).replace('T', ' ')} ${visibleControls(note.author)}${note.kind === 'change' ? ' ·' : ':'}`;
        lines.push(...`  - ${heading} ${indented(note.text, '    ')}`.split('\n').map((line) => line.trimEnd()));
      }
    }
    lines.push('');
  }
  return lines.join('\n');
}

export const isState = (word: string): word is State => (STATES as readonly string[]).includes(word);
