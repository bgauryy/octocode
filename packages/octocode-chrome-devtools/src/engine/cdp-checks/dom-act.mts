// One DOM step end to end: locate, act (trusted input when pending), settle, then verify the effect.
import { waitForPageReady } from '../dom-actionability.mjs';
import { DONE } from './dom-config.mjs';
import { locateWhenReady } from './dom-locate.mjs';
import {
  PAGE_STATE_JS,
  TRACE_STOP_FN,
  VERIFY_FN,
  textWaitFn,
} from './dom-page-scripts.mjs';
import { performTrusted } from './dom-pointer.mjs';
import { setTimeout as sleep } from 'node:timers/promises';
import type { Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import type {
  DomConfig,
  DomContext,
  DomEffects,
  DomStep,
  LocatedStep,
  StepDetails,
} from './dom-types.mjs';

/** The state field an action must change, and the value it must reach. */
function expectation(
  step: DomStep
): { field: string; want: unknown; loose?: boolean } | null {
  if (step.action === 'fill' || step.action === 'type')
    return { field: 'value', want: step.value };
  if (step.action === 'check' || step.action === 'uncheck')
    return { field: 'checked', want: step.action === 'check' };
  if (step.action === 'select')
    return { field: 'selected', want: step.value, loose: true };
  if (step.action === 'upload')
    return {
      field: 'files',
      want: step.value
        .split('|')
        .map(f => f.trim().split('/').pop())
        .filter(Boolean)
        .join(', '),
    };
  return null;
}

async function readText(
  cdp: Session,
  fn: string,
  objectId: string | null | undefined
) {
  const res = objectId
    ? await cdp
        .send('Runtime.callFunctionOn', {
          objectId,
          functionDeclaration: fn,
          returnByValue: true,
        })
        .catch(() => null)
    : null;
  if (res?.result) return res;
  return cdp
    .send('Runtime.evaluate', {
      expression: `(${fn}).call(document)`,
      returnByValue: true,
    })
    .catch(() => null);
}

/** Waits for any of the '|'-separated texts (or the literal text) near the element or in the page. */
export async function waitForText(
  cdp: Session,
  text: string,
  { cfg, log }: DomContext,
  objectId?: string | null
) {
  const wanted = cfg.literalWaitText
    ? [text]
    : text
        .split('|')
        .map(t => t.trim())
        .filter(Boolean);
  const start = Date.now();
  let lastProgress = -Infinity;
  const fn = textWaitFn(wanted);
  while (Date.now() - start < cfg.waitMs) {
    const hit = (await readText(cdp, fn, objectId))?.result?.value;
    if (hit) return { hit, ms: Date.now() - start };
    if (Date.now() - start - lastProgress >= 1000) {
      lastProgress = Date.now() - start;
      log(`[PROGRESS] waiting content elapsedMs=${lastProgress}`);
    }
    await sleep(150);
  }
  return { hit: null, ms: Date.now() - start };
}

async function stateAfter(
  cdp: Session,
  objectId: string | null,
  navigated: string | null
) {
  const res =
    !navigated && objectId
      ? await cdp
          .send('Runtime.callFunctionOn', {
            objectId,
            functionDeclaration: VERIFY_FN,
            returnByValue: true,
          })
          .catch(() => null)
      : null;
  return (
    res?.result?.value ??
    (
      await cdp
        .send('Runtime.evaluate', {
          expression: PAGE_STATE_JS,
          returnByValue: true,
        })
        .catch(() => null)
    )?.result?.value ??
    null
  );
}

function verifyExpectation(step: DomStep, details: StepDetails) {
  const exp = expectation(step);
  if (!exp) return;
  const got = details.after?.state?.[exp.field];
  const ok = exp.loose
    ? String(got ?? '')
        .toLowerCase()
        .includes(String(exp.want).toLowerCase()) || got === exp.want
    : got === exp.want;
  details.verified = ok;
  details.expected = { [exp.field]: exp.want, got };
}

async function settleAndVerify(
  cdp: Session,
  step: DomStep,
  details: StepDetails,
  {
    objectId,
    effects,
    cfg,
  }: { objectId: string | null; effects: DomEffects; cfg: DomConfig }
) {
  await sleep(cfg.settleMs);
  if (effects.navigatedTo) await waitForPageReady(cdp, 8000);
  details.after = await stateAfter(cdp, objectId, effects.navigatedTo);
  details.effects = { ...effects };
  verifyExpectation(step, details);
}

async function actPending(
  cdp: Session,
  step: DomStep,
  details: StepDetails,
  { objectId, log }: { objectId: string | null; log: DomContext['log'] }
) {
  details.pending = false;
  try {
    details.operation = await performTrusted(cdp, step, details, objectId);
  } catch (error) {
    details.operation = 'input-failed';
    details.error = errorMessage(error);
    log(`[FINDING] DOM_INPUT_FAILED ${details.error}`);
  }
}

/** Locates, acts and verifies one step; effects collects navigation, dialog and popup events. */
export async function runStep(
  cdp: Session,
  step: DomStep,
  effects: DomEffects,
  ctx: DomContext
): Promise<LocatedStep> {
  const { label, details, objectId } = await locateWhenReady(cdp, step, ctx);
  if (details.pending)
    await actPending(cdp, step, details, { objectId, log: ctx.log });
  details.acted = details.operation === DONE[step.action];
  if (details.acted)
    await settleAndVerify(cdp, step, details, {
      objectId,
      effects,
      cfg: ctx.cfg,
    });
  if (ctx.cfg.traceEvents && !details.acted && objectId) {
    const traced = await cdp.send('Runtime.callFunctionOn', {
      objectId,
      functionDeclaration: TRACE_STOP_FN,
      returnByValue: true,
    });
    details.events = traced.result?.value || [];
  }
  return { label, details, objectId };
}
