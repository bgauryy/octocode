// Plan step conditions: wait until the page is ready and matches an exact URL, visible selector and/or text.
import { setTimeout as sleep } from 'node:timers/promises';

/** The message of a Runtime exceptionDetails payload. */
export const exceptionMessage = details =>
  details.exception?.description || details.text;

const conditionExpression = condition => `(() => {
        const c = ${JSON.stringify(condition)};
        const visible = el => { const r = el.getBoundingClientRect(), s = getComputedStyle(el); return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden' && Number(s.opacity || '1') > 0; };
        return { ready: document.readyState !== 'loading', url: location.href,
          matched: (!c.url || location.href === c.url) && (!c.selector || [...document.querySelectorAll(c.selector)].some(visible)) && (!c.text || document.body?.innerText.includes(c.text)) };
      })()`;

// A navigation that replaces the context reads as "not yet"; any other failure stops the wait.
async function evaluateCondition(cdp, condition) {
  try {
    return await cdp.send('Runtime.evaluate', {
      returnByValue: true,
      expression: conditionExpression(condition),
    });
  } catch (error) {
    if (
      !/context.*destroyed|Cannot find context|Inspected target navigated/i.test(
        error.message
      )
    )
      throw error;
    return undefined;
  }
}

/** Polls until the condition holds; returns { ready, url, matched, elapsedMs } or throws on timeout. */
export async function waitCondition(cdp, condition, timeoutMs, log) {
  const started = Date.now();
  let progress = 0;
  do {
    const response = await evaluateCondition(cdp, condition);
    if (response?.exceptionDetails)
      throw new Error(exceptionMessage(response.exceptionDetails));
    if (response?.result?.value?.ready && response.result.value.matched)
      return { ...response.result.value, elapsedMs: Date.now() - started };
    if (Date.now() - started >= progress) {
      log(
        `[PROGRESS] waiting condition=${JSON.stringify(condition)} elapsedMs=${Date.now() - started}`
      );
      progress += 1000;
    }
    await sleep(100);
  } while (Date.now() - started < timeoutMs);
  throw new Error(`Condition timed out: ${JSON.stringify(condition)}`);
}
