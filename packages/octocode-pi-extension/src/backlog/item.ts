import { storedSecret, storedText } from '../shared/sanitize.js';
import { cutText } from '../shared/util.js';
import { refText } from './row.js';

/** A backlog item and the rules for its stored form: states, priorities, refs, cleaned fields and the change note an update writes. */

export const STATES = ['backlog', 'todo', 'ongoing', 'done'] as const;
export type State = (typeof STATES)[number];
export const PRIORITIES = ['p0', 'p1', 'p2', 'p3'] as const;
type Priority = (typeof PRIORITIES)[number];

export interface Item {
  /** Database row id (notes hang from it). */
  rowId: number;
  seq: number;
  /** `B<seq>`. */
  ref: string;
  title: string;
  /** What done means: the observable result or check that closes the item. */
  goal: string;
  body: string;
  /** Repo-relative files, directories or components the work touches. */
  paths: string[];
  /** Why it matters: the problem and who it affects. */
  why: string;
  /** Observations that justify it: `path:line — what it shows`, a command and its result, or a link. */
  evidence: string[];
  /** The intended approach and steps. */
  plan: string;
  state: State;
  priority: number;
  tags: string[];
  assignee?: string;
  createdBy: string;
  sourceSession?: string;
  createdAt: number;
  updatedAt: number;
  /** When the item was last claimed (moved to ongoing, or taken over); kept when it is done, cleared when released. */
  startedAt?: number;
  doneAt?: number;
  /** The item this one came from, by number (`B<from>`). */
  from?: number;
  version: number;
}

/** `note`: text an agent or the user wrote; `change`: what an update changed, written by the store. */
export type NoteKind = 'note' | 'change';

export interface Note {
  at: number;
  author: string;
  kind: NoteKind;
  text: string;
}

/** Who is acting: the id written as assignee and note author, and for a subagent the sessions of its parent. */
export interface Actor {
  id: string;
  /** Set for a subagent: its parent's sessions, the only assignees whose items it may update (it never claims). */
  owners?: ReadonlySet<string>;
}

export class BacklogError extends Error {}

const tagText = (tags: readonly string[]) => (tags.length ? tags.map((tag) => `#${tag}`).join(' ') : 'none');
const listText = (entries: readonly string[]) => (entries.length ? entries.join(', ') : 'none');

/** A text field's change: the old text is kept in full, so a replacement loses nothing. */
const replaced = (name: string, before: string) => (before ? `${name} replaced; it was:\n${before}` : `${name} added`);

/** The change note for one update: every field that changed, old → new; a replaced text is kept in full. */
export function changeText(before: Item, after: Item): string {
  return [...fieldChanges(before, after), ...textChanges(before, after)].join(' · ');
}

/** The short fields that changed, each old → new (a title or goal names only the old one). */
function fieldChanges(before: Item, after: Item): string[] {
  const parts: string[] = [];
  if (after.state !== before.state) parts.push(`state ${before.state} → ${after.state}`);
  if (after.assignee !== before.assignee) parts.push(`assignee ${before.assignee ?? 'none'} → ${after.assignee ?? 'none'}`);
  if (after.priority !== before.priority) parts.push(`priority ${priorityName(before.priority)} → ${priorityName(after.priority)}`);
  if (after.tags.join(' ') !== before.tags.join(' ')) parts.push(`tags ${tagText(before.tags)} → ${tagText(after.tags)}`);
  if (after.from !== before.from) parts.push(`from ${refText(before.from)} → ${refText(after.from)}`);
  if (after.title !== before.title) parts.push(`title was "${before.title}"`);
  if (after.goal !== before.goal) parts.push(before.goal ? `goal was "${before.goal}"` : 'goal added');
  if (after.paths.join('\n') !== before.paths.join('\n')) parts.push(`paths ${listText(before.paths)} → ${listText(after.paths)}`);
  return parts;
}

/** The long text fields that changed, each with its old text in full. */
function textChanges(before: Item, after: Item): string[] {
  const parts: string[] = [];
  if (after.why !== before.why) parts.push(replaced('why', before.why));
  if (after.evidence.join('\n') !== before.evidence.join('\n')) parts.push(replaced('evidence', before.evidence.join('\n')));
  if (after.plan !== before.plan) parts.push(replaced('plan', before.plan));
  if (after.body !== before.body) parts.push(replaced('body', before.body));
  return parts;
}

/** A title as stored: one line (a title is a one-line summary; a newline would break lists, the board and the export). */
export const storedTitle = (title: string | undefined): string => storedText(title).replace(/\s+/g, ' ');

/** `B12`, `b12`, `#12` or `12` → 12. */
export function parseRef(ref: string): number | undefined {
  const match = /^\s*(?:[bB]|#)?(\d{1,9})\s*$/.exec(ref);
  return match ? Number(match[1]) : undefined;
}

/** Tags as stored: lower-case words of letters, digits, `.`, `_`, `-`, without a leading `#`, deduplicated. */
export function cleanTags(tags: readonly string[]): string[] {
  const words = tags.map((tag) => cutText(tag.trim().replace(/^#+/, '').toLowerCase().replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, ''), 32, { chars: true })).filter(Boolean);
  return [...new Set(words)];
}

/** A list as stored: one entry per line, each entry one trimmed line, empty entries and repeats dropped. */
export function cleanList(entries: readonly string[]): string[] {
  return [...new Set(entries.map((entry) => storedText(entry).replace(/\s+/g, ' ')).filter(Boolean))];
}

/**
 * Refuses text that looks like a credential (the backlog is plain text in the home database). Checked on the
 * sanitized text, the form that is stored, so an invisible character cannot split a token past the check.
 */
export function refuseSecrets(fields: Record<string, string | readonly string[] | undefined>): void {
  const found = storedSecret(fields);
  if (found) throw new BacklogError(`Not stored: the ${found.name} ${found.problem.replace(/^it /, '')}.`);
}

export const priorityName = (priority: number): Priority => PRIORITIES[priority] ?? 'p2';

export function clampPriority(priority: number | undefined): number {
  return priority === undefined || !Number.isInteger(priority) ? 2 : Math.min(3, Math.max(0, priority));
}
