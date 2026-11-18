// History and reload plan steps: move the page through its session history or reload it, then
// wait for the document and the optional `after` postcondition. Neither step is ever replayed.
import { waitCondition } from './plan-condition.mjs';
import type { Handler } from '../cdp-connection.mjs';
import type { PlanOp, StepContext } from './plan-types.mjs';

type HistoryEntry = { id: number; url: string };

const DIRECTION_OFFSET = { back: -1, forward: 1 } as const;

/**
 * Runs `trigger` and resolves once the root frame commits a new document or finishes a
 * same-document navigation (hash or pushState), whichever the browser reports.
 */
async function navigateAndWait(
  { step, timeout, row, target }: StepContext,
  trigger: () => Promise<unknown>,
  state: { log: (line: string) => void }
) {
  await target.send('Page.enable');
  const { frame } = (await target.send('Page.getFrameTree')).frameTree;
  const { promise: settled, resolve } = Promise.withResolvers<boolean>();
  const onDocument: Handler = ({ frame: navigated }) => {
    if (!navigated.parentId) resolve(true);
  };
  const onSameDocument: Handler = ({ frameId }) => {
    if (frameId === frame.id) resolve(true);
  };
  target.on('Page.frameNavigated', onDocument);
  target.on('Page.navigatedWithinDocument', onSameDocument);
  const timer = setTimeout(() => resolve(false), timeout);
  try {
    await trigger();
    if (!(await settled))
      throw Object.assign(
        new Error(`${step.op} did not navigate within timeout`),
        { outcomeUncertain: true }
      );
    const condition = await waitCondition(
      target,
      step.after || {},
      timeout,
      state.log
    );
    row.condition = condition;
    target.targetInfo = { ...target.targetInfo, url: condition.url };
  } finally {
    clearTimeout(timer);
    target.off('Page.frameNavigated', onDocument);
    target.off('Page.navigatedWithinDocument', onSameDocument);
  }
}

const history: PlanOp = (state, current) => {
  const { step, target } = current;
  const direction = step.direction as keyof typeof DIRECTION_OFFSET;
  return navigateAndWait(
    current,
    async () => {
      const { currentIndex, entries } = (await target.send(
        'Page.getNavigationHistory'
      )) as { currentIndex: number; entries: HistoryEntry[] };
      const entry = entries[currentIndex + DIRECTION_OFFSET[direction]];
      if (!entry) throw new Error(`No ${direction} history entry`);
      await target.send('Page.navigateToHistoryEntry', { entryId: entry.id });
    },
    state
  );
};

const reload: PlanOp = (state, current) =>
  navigateAndWait(
    current,
    () =>
      current.target.send('Page.reload', {
        ignoreCache: current.step.ignoreCache === true,
      }),
    state
  );

export const historyOps: Record<string, PlanOp> = { history, reload };
