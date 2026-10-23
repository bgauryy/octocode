// Page-level plan steps: navigation, DOM actions and waits, and element extraction.
// Each op takes the plan run state and the current step { step, index, timeout, row, target }.
import { createDomOperations } from './dom-operations-check.mjs';
import { exceptionMessage, waitCondition } from './plan-condition.mjs';

async function goto(state, { step, timeout, row, target }) {
  await target.send('Page.enable');
  let handler, timer;
  const committed = new Promise(resolve => {
    handler = ({ frame }) => {
      if (!frame.parentId) resolve(true);
    };
    target.on('Page.frameNavigated', handler);
    timer = setTimeout(() => resolve(false), timeout);
  });
  try {
    const nav = await target.send('Page.navigate', { url: step.url });
    if (nav.errorText) throw new Error(nav.errorText);
    if (nav.loaderId && !(await committed))
      throw new Error('Navigation did not commit within timeout');
    row.condition = await waitCondition(
      target,
      step.after || {},
      timeout,
      state.log
    );
  } finally {
    clearTimeout(timer);
    target.off('Page.frameNavigated', handler);
  }
  target.targetInfo = { ...target.targetInfo, url: row.condition.url };
}

// After a DOM step: act already verified its text, so only selector/url remain to wait for.
async function domCondition(state, { step, timeout, target }) {
  const condition =
    step.op === 'act'
      ? Object.fromEntries(
          Object.entries(step.after).filter(([key]) => key !== 'text')
        )
      : step.after;
  return {
    ...(step.after.text ? { text: step.after.text, verified: true } : {}),
    ...(Object.keys(condition).length
      ? await waitCondition(target, condition, timeout, state.log)
      : {}),
  };
}

async function domStep(state, current) {
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
  if (step.after) row.condition = await domCondition(state, current);
}

const extractExpression = step => `(() => {
            const fields = ${JSON.stringify(step.fields || ['text', 'href'])};
            const rows = [...document.querySelectorAll(${JSON.stringify(step.selector)})].map(el => Object.fromEntries(fields.map(field => [field,
              field === 'text' ? el.innerText || el.textContent || '' : field === 'href' ? el.href || null : field === 'value' ? el.value ?? null : field === 'role' ? el.getAttribute('role') : el.getAttribute('aria-label') || el.getAttribute('name') || null])));
            return {rows, url: location.href, title: document.title};
          })()`;

function extractedRows({ step, row, target }, value) {
  const rows = Array.isArray(value) ? value : value?.rows;
  if (typeof value?.url === 'string') {
    row.source = { ...row.source, url: value.url, title: value.title };
    target.targetInfo = { ...target.targetInfo, url: value.url };
  }
  if (!Array.isArray(rows)) throw new Error('Extraction returned no rows');
  if (!rows.length && !step.allowEmpty)
    throw new Error(
      'Extraction matched zero elements; use allowEmpty for an expected empty result'
    );
  return rows;
}

async function extract(state, current) {
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

export const pageOps = { goto, act: domStep, wait: domStep, extract };
