import path from 'node:path';
import { existingLibrary } from '../shared/archive.js';
import { fenceUntrusted, headerText } from '../shared/fence.js';
import { shortPath } from '../shared/format.js';
import { emptyTotals } from '../shared/util.js';
import { asksNote, type Ask } from '../team/asks.js';
import { archiveNote, compactionNote, noticeLines, sessionNote, type RunDetails } from './render.js';
import type { Run } from './claim.js';
import { childSessionFile, type ChildSession } from './launch.js';
import { GENERAL_PROFILE } from './profiles.js';
import type { runSubagent } from './process.js';

/** What a finished child run returns: its report text, its usage, and its error when its model call failed. */
export type Outcome = Awaited<ReturnType<typeof runSubagent>>;

/** Who stopped the runs of a permission change: their reports say so, so the model asks before starting them again. */
export const PERMISSION_STOP = 'a permission change';

/** The run's trusted footer lines: its compactions, its questions to the parent, and where its conversation is saved when it failed. */
export const runFooter = (details: RunDetails, failed: boolean): string[] => [compactionNote(details), archiveNote(details), asksNote(details.asks), failed ? sessionNote(details) : ''];

/**
 * Once the child's run ends (however), the run's details name its transcript file (Pi chose the file's name) and list
 * its questions to the parent with their answers (`asks`: read from the team's messages, so no one has to report them).
 */
export function recordRun<T>(details: RunDetails, session: ChildSession, asks: () => Ask[], run: Promise<T>): Promise<T> {
  return run.finally(() => {
    const file = childSessionFile(session);
    const archive = existingLibrary(session.id);
    if (file || archive) details.session = { id: session.id, ...(file ? { file } : {}), ...(archive ? { archive: path.dirname(archive) } : {}) };
    const questions = asks();
    if (questions.length > 0) details.asks = questions;
  });
}

/** The row of a call waiting for a slot: who it will be, and how many run before it. */
export function waitingUpdate(profile: string | undefined, goal: string, running: number): { content: Array<{ type: 'text'; text: string }>; details: RunDetails } {
  const details = startDetails({ id: '', profile: profile ?? GENERAL_PROFILE, goal, status: 'waiting' });
  return { content: [{ type: 'text', text: `waiting for a slot (${running} running)` }], details };
}

/** A run's details before it has spent anything. */
const startDetails = (fields: Pick<RunDetails, 'id' | 'profile' | 'goal' | 'status'>): RunDetails => ({ ...fields, toolCalls: 0, activity: [], ...emptyTotals(), startedAt: Date.now() });

/** A run's details as it starts. */
export function runDetails(run: Run): RunDetails {
  const { model, fallback } = run.route;
  return { ...startDetails({ id: run.identity.id, profile: run.identity.role, goal: run.identity.goal, status: 'running' }), session: { id: run.identity.id }, ...(model ? { model } : {}), ...(fallback ? { fallback } : {}) };
}

export const withShot = (details: RunDetails, text: string) => (details.shot ? `${text}\n\n(Last screenshot: ${shortPath(details.shot)})` : text);

/**
 * A background report as the model reads it: the header and footer are this process's own words (the agent's name, role
 * and goal, its run statistics, what the parent did), and the child's text sits in the shared fence for untrusted text,
 * so a report cannot pose as the user, a teammate or the system.
 */
export function reportMessage(header: string, body: string, footer: string[] = []): string {
  const tail = footer.map((part) => part.trim()).filter(Boolean).join('\n\n');
  return fenceUntrusted(headerText(header, 400), body, tail ? { footer: tail } : {});
}

/** A foreground result's lines about the child itself (no fence there): its notices, its compactions, and where its conversation is saved when it failed. */
export const foregroundExtras = (details: RunDetails, failed: boolean): string => {
  const lines = [...noticeLines(details), ...runFooter(details, failed)].filter(Boolean);
  return lines.length > 0 ? `\n\n${lines.join('\n')}` : '';
};

/** What the parent does after a failed subagent; its partial answer, if any, was never checked. */
export const FAILED_NEXT = 'Retry with a narrower task or do the work yourself.';
export const PARTIAL_UNVERIFIED = ' The partial answer is unverified.';
