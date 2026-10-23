import { storedSecret } from '../shared/sanitize.js';
import { DAY_MS } from '../shared/util.js';
import { refText } from './row.js';
import { BacklogError, priorityName, STATES, type Item, type Note, type State } from './item.js';
import type { BacklogStore } from './store.js';

/** An item idle for longer than this (and not done) is listed under Attention in the page. */
const IDLE_MS = 7 * DAY_MS;

interface SnapshotItem {
  ref: string;
  title: string;
  goal: string;
  body: string;
  paths: string[];
  why: string;
  evidence: string[];
  plan: string;
  state: State;
  priority: string;
  tags: string[];
  assignee?: string;
  /** Whether the assignee is a running session at snapshot time (the writer's own session counts). */
  assigneeLive?: boolean;
  createdBy: string;
  sourceSession?: string;
  createdAt: number;
  updatedAt: number;
  startedAt?: number;
  doneAt?: number;
  version: number;
  /** The ref of the item this one came from. */
  from?: string;
  /** The refs of the items that came from this one. */
  derived: string[];
  /** Every note and change, oldest first. */
  notes: Note[];
}

/** The whole backlog of one repository as the HTML page embeds it: every item in every state, with every note. */
export interface Snapshot {
  repo: string;
  generatedAt: number;
  states: readonly State[];
  idleMs: number;
  items: SnapshotItem[];
}

function snapshotItem(store: BacklogStore, item: Item, self: string | undefined): SnapshotItem {
  return {
    ref: item.ref,
    title: item.title,
    goal: item.goal,
    body: item.body,
    paths: item.paths,
    why: item.why,
    evidence: item.evidence,
    plan: item.plan,
    state: item.state,
    priority: priorityName(item.priority),
    tags: item.tags,
    ...(item.assignee ? { assignee: item.assignee, assigneeLive: item.assignee === self || store.liveHolder(item.assignee) !== undefined } : {}),
    createdBy: item.createdBy,
    ...(item.sourceSession ? { sourceSession: item.sourceSession } : {}),
    createdAt: item.createdAt,
    updatedAt: item.updatedAt,
    ...(item.startedAt ? { startedAt: item.startedAt } : {}),
    ...(item.doneAt ? { doneAt: item.doneAt } : {}),
    version: item.version,
    ...(item.from === undefined ? {} : { from: refText(item.from) }),
    derived: store.derived(item).map((child) => child.ref),
    // A negative limit is no limit: the page keeps every note in full.
    notes: store.notes(item, -1),
  };
}

/**
 * Refuses a snapshot with a credential-looking string in any of its text, naming the item. Writes are already refused
 * (`refuseSecrets`); this checks again what the page will carry, so text stored before that check, or by another
 * build, never lands in a file that is easy to share.
 */
function refuseSecrets(snapshot: Snapshot): void {
  const found = storedSecret({ repository: snapshot.repo });
  if (found) throw new BacklogError(`Not written: the repository name ${found.problem.replace(/^it /, '')}.`);
  for (const item of snapshot.items) {
    const { title, goal, body, paths, why, evidence, plan, tags, assignee, createdBy, sourceSession, notes } = item;
    const noteFields = notes.flatMap((note, index) => [[`note ${index + 1}`, note.text], [`note ${index + 1} author`, note.author]] as const);
    const hit = storedSecret({ title, goal, body, paths, why, evidence, plan, tags, assignee, createdBy, sourceSession, ...Object.fromEntries(noteFields) });
    if (hit) throw new BacklogError(`Not written: the ${hit.name} of ${item.ref} ${hit.problem.replace(/^it /, '')}. Remove it from the item, or delete the item, and run /backlog html again.`);
  }
}

/** Every item of the store, in board order, with its notes and derivation links; throws when any text looks like a secret. */
export function backlogSnapshot(store: BacklogStore, repo: string, now: number, self?: string): Snapshot {
  const snapshot: Snapshot = { repo, generatedAt: now, states: STATES, idleMs: IDLE_MS, items: store.list().items.map((item) => snapshotItem(store, item, self)) };
  refuseSecrets(snapshot);
  return snapshot;
}
