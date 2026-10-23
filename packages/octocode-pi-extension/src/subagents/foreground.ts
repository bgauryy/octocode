import { USER_SENDER } from '../shared/messages.js';
import { errorMessage } from '../shared/util.js';
import type { RunDetails } from './render.js';
import type { AgentRuns, Run } from './claim.js';
import type { BackgroundRun } from './command.js';
import { USER_STOP } from './process.js';
import { reportStatus } from './report.js';
import { FAILED_NEXT, foregroundExtras, PARTIAL_UNVERIFIED, PERMISSION_STOP, withShot, type Outcome } from './run-details.js';

/** How a foreground run starts, ends and shows that it is stopping. */
export interface ForegroundRun {
  launch: (signal: AbortSignal) => Promise<Outcome>;
  finish: () => Promise<string>;
  warning: string;
  signal: AbortSignal | undefined;
  stopping: () => void;
}

/** Why a foreground run that someone stopped ended: who stopped it, so the model does not just start it again. */
function stoppedError(id: string, by: string): Error {
  if (by === USER_SENDER) return new Error(`Subagent ${id} was stopped by the user.`);
  return new Error(`Subagent ${id} was stopped by ${by}.${by === PERMISSION_STOP ? ' Ask the user before starting it again.' : ''}`);
}

/**
 * Runs the subagent and waits for its report; the child's model usage counts toward this session's totals. `stopping`
 * redraws the row the moment a stop begins: the child, and an isolated run's worktree, still take a moment to close.
 */
export async function runForeground(runs: AgentRuns, run: Run, details: RunDetails, { launch, finish, warning, signal, stopping }: ForegroundRun) {
  const { identity } = run;
  // Its own controller, so `/agents kill` can stop this run without aborting the parent's turn. Esc is the user's stop.
  const controller = new AbortController();
  const entry: BackgroundRun = { controller, done: Promise.resolve() };
  const abort = () => controller.abort(USER_STOP);
  controller.signal.addEventListener('abort', stopping, { once: true });
  if (signal?.aborted) abort();
  signal?.addEventListener('abort', abort, { once: true });
  runs.foreground.set(identity.id, entry);
  let outcome: Outcome;
  try {
    outcome = await launch(controller.signal);
  } catch (error) {
    const note = `${await finish()}${entry.stoppedBy ? '' : foregroundExtras(details, true)}`;
    const reason = entry.stoppedBy ? stoppedError(identity.id, entry.stoppedBy) : error;
    throw note ? new Error(`${errorMessage(reason)}${note}`) : reason;
  } finally {
    signal?.removeEventListener('abort', abort);
    runs.foreground.delete(identity.id);
  }
  const note = `${await finish()}${warning}${foregroundExtras(details, Boolean(outcome.error))}`;
  if (outcome.error) {
    // An error result rather than a throw, so the child's spent tokens still reach this session's totals.
    details.status = 'failed';
    details.outcome = 'failed';
    const text = `Subagent ${identity.id} failed: ${outcome.error}\n${FAILED_NEXT}${outcome.text ? `${PARTIAL_UNVERIFIED}\n\nPartial answer:\n${outcome.text}` : ''}${note}`;
    return { isError: true, content: [{ type: 'text' as const, text }], details, usage: outcome.usage };
  }
  details.status = 'done';
  const reported = reportStatus(outcome.text);
  if (reported) details.outcome = reported;
  return { content: [{ type: 'text' as const, text: `${withShot(details, outcome.text || '(subagent returned no text)')}${note}` }], details, usage: outcome.usage };
}
