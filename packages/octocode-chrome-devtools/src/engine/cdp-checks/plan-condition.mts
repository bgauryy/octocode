// Plan step conditions: wait until the page is ready and matches an exact URL, visible selector and/or text.
import { setTimeout as sleep } from 'node:timers/promises';
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import type { ConditionResult, PageCondition } from './plan-types.mjs';

/** The message of a Runtime exceptionDetails payload. */
export const exceptionMessage = (details: Payload): string =>
  details.exception?.description || details.text;

const conditionExpression = (condition: PageCondition) => `(() => {
        const c = ${JSON.stringify(condition)};
        const visible = el => { const r = el.getBoundingClientRect(), s = getComputedStyle(el); return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden' && Number(s.opacity || '1') > 0; };
        return { ready: document.readyState !== 'loading', url: location.href, title: document.title,
          matched: (!c.url || location.href === c.url) && (!c.selector || [...document.querySelectorAll(c.selector)].some(visible)) && (!c.text || document.body?.innerText.includes(c.text)) };
      })()`;

// A navigation that replaces the context reads as "not yet"; any other failure stops the wait.
async function evaluateCondition(
  cdp: Pick<Session, 'send'>,
  condition: PageCondition
) {
  try {
    return await cdp.send('Runtime.evaluate', {
      returnByValue: true,
      expression: conditionExpression(condition),
    });
  } catch (error) {
    if (
      !/context.*destroyed|Cannot find context|Inspected target navigated/i.test(
        errorMessage(error)
      )
    )
      throw error;
    return undefined;
  }
}

/** Where the page was at the last poll, for the timeout message. */
const pageState = (value: { url?: string; title?: string; ready?: boolean }) =>
  `; page was ${value.ready ? 'ready' : 'still loading'} at ${JSON.stringify(value.url ?? '')} titled ${JSON.stringify(value.title ?? '')}`;

/** Polls until the condition holds; returns { ready, url, matched, elapsedMs } or throws on timeout naming the page state. */
export async function waitCondition(
  cdp: Pick<Session, 'send'>,
  condition: PageCondition,
  timeoutMs: number,
  log: (line: string) => void
): Promise<ConditionResult> {
  const started = Date.now();
  let progress = 0;
  let observed: { url?: string; title?: string; ready?: boolean } | undefined;
  do {
    const response = await evaluateCondition(cdp, condition);
    if (response?.exceptionDetails)
      throw new Error(exceptionMessage(response.exceptionDetails));
    const { title, ...state } = response?.result?.value ?? {};
    if (response?.result?.value) observed = { ...state, title };
    if (state.ready && state.matched)
      return { ...state, elapsedMs: Date.now() - started };
    if (Date.now() - started >= progress) {
      log(
        `[PROGRESS] waiting condition=${JSON.stringify(condition)} elapsedMs=${Date.now() - started}`
      );
      progress += 1000;
    }
    await sleep(100);
  } while (Date.now() - started < timeoutMs);
  throw new Error(
    `Condition timed out: ${JSON.stringify(condition)}${observed ? pageState(observed) : ''}`
  );
}
