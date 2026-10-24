// Page-level plan steps: navigation, DOM actions and waits, and element extraction.
// Each op takes the plan run state and the current step { step, index, timeout, row, target }.
import { createDomOperations } from './dom-operations-check.mjs';
import { exceptionMessage, waitCondition } from './plan-condition.mjs';
import type { Handler } from '../cdp-connection.mjs';
import type {
  PageCondition,
  PlanOp,
  PlanState,
  PlanStep,
  StepContext,
} from './plan-types.mjs';

/** What the extraction expression returns: rows plus the page it read them from. */
type ExtractedPage = { rows?: unknown; url?: unknown; title?: unknown };

async function goto(
  state: PlanState,
  { step, timeout, row, target }: StepContext
) {
  await target.send('Page.enable');
  const { promise: committed, resolve } = Promise.withResolvers<boolean>();
  const handler: Handler = ({ frame }) => {
    if (!frame.parentId) resolve(true);
  };
  target.on('Page.frameNavigated', handler);
  const timer = setTimeout(() => resolve(false), timeout);
  let condition;
  try {
    const nav = await target.send('Page.navigate', { url: step.url });
    if (nav.errorText) throw new Error(nav.errorText);
    if (nav.loaderId && !(await committed))
      throw new Error('Navigation did not commit within timeout');
    condition = await waitCondition(
      target,
      step.after || {},
      timeout,
      state.log
    );
    row.condition = condition;
  } finally {
    clearTimeout(timer);
    target.off('Page.frameNavigated', handler);
  }
  target.targetInfo = { ...target.targetInfo, url: condition.url };
}

// After a DOM step: act already verified its text, so only selector/url remain to wait for.
async function domCondition(
  state: PlanState,
  { step, timeout, target }: StepContext,
  after: PageCondition
) {
  const condition =
    step.op === 'act'
      ? Object.fromEntries(
          Object.entries(after).filter(([key]) => key !== 'text')
        )
      : after;
  return {
    ...(after.text ? { text: after.text, verified: true } : {}),
    ...(Object.keys(condition).length
      ? await waitCondition(target, condition, timeout, state.log)
      : {}),
  };
}

async function domStep(state: PlanState, current: StepContext) {
  const { step, index, timeout, row, target } = current;
  const { plan } = state;
  const act = step.op === 'act';
  const result = await createDomOperations({
    steps: [{ ...step, action: act ? step.action : 'wait' }],
    waitMs: timeout,
    settleMs: plan.settleMs ?? 0,
    waitText: act ? (step.after?.text ?? '') : '',
    traceEvents: plan.traceEvents ?? false,
    literalWaitText: true,
    diff: false,
    artifactName: `dom-step-${index + 1}.json`,
  }).run(target);
  row.artifact = result.artifactPath;
  if (!result.ok)
    throw new Error('DOM action or wait failed; action will not be repeated');
  if (step.after)
    row.condition = await domCondition(state, current, step.after);
}

const extractExpression = (step: PlanStep) => `(() => {
            const fields = ${JSON.stringify(step.fields || ['text', 'href'])};
            const rows = [...document.querySelectorAll(${JSON.stringify(step.selector)})].map(el => Object.fromEntries(fields.map(field => [field,
              field === 'text' ? el.innerText || el.textContent || '' : field === 'href' ? el.href || null : field === 'value' ? el.value ?? null : field === 'role' ? el.getAttribute('role') : el.getAttribute('aria-label') || el.getAttribute('name') || null])));
            return {rows, url: location.href, title: document.title};
          })()`;

function extractedRows(
  { step, row, target }: StepContext,
  value: ExtractedPage | unknown[] | undefined
) {
  const page = Array.isArray(value) ? undefined : value;
  const rows = Array.isArray(value) ? value : page?.rows;
  if (typeof page?.url === 'string') {
    row.source = { ...row.source, url: page.url, title: page.title };
    target.targetInfo = { ...target.targetInfo, url: page.url };
  }
  if (!Array.isArray(rows)) throw new Error('Extraction returned no rows');
  if (!rows.length && !step.allowEmpty)
    throw new Error(
      'Extraction matched zero elements; use allowEmpty for an expected empty result'
    );
  return rows;
}

async function extract(state: PlanState, current: StepContext) {
  const { step, index, timeout, row, target } = current;
  if (step.after)
    row.condition = await waitCondition(target, step.after, timeout, state.log);
  const evaluated = await target.send('Runtime.evaluate', {
    returnByValue: true,
    expression: extractExpression(step),
  });
  if (evaluated.exceptionDetails) {
    row.artifact = state.cdp.saveArtifact(
      `extract-error-${index + 1}.json`,
      evaluated
    ).file;
    throw new Error(exceptionMessage(evaluated.exceptionDetails));
  }
  const rows = extractedRows(current, evaluated.result?.value);
  row.count = rows.length;
  row.artifact = state.cdp.saveArtifact(`extract-${index + 1}.json`, rows).file;
}

export const pageOps: Record<string, PlanOp> = {
  goto,
  act: domStep,
  wait: domStep,
  extract,
};
