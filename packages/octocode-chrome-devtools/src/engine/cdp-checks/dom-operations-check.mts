import { writeFileSync } from 'fs';
import { join } from 'path';
import { waitForPageReady } from '../dom-actionability.mjs';
import { collectRefs } from '../ax-snapshot.mjs';
import { readDomConfig, readSteps } from './dom-config.mjs';
import { locateWhenReady } from './dom-locate.mjs';
import { runStep, waitForText } from './dom-act.mjs';
import { recordNewRefs } from './dom-ref-diff.mjs';

// Env (one step):
//   DOM_REF | DOM_SELECTOR  target (snapshot ref, or CSS; default first control)
//   DOM_ACTION    inspect | click | dblclick | fill | type | press | hover | select | check | uncheck |
//                 focus | scroll | upload | drag | wait
//   DOM_VALUE     fill/type text, select option, scroll deltaY, upload paths ('|'-separated), wait text
//   DOM_KEY       press: Enter, Tab, Escape, ArrowDown, a, Control+A, Shift+Tab…
//   DOM_TO_REF | DOM_TO_SELECTOR  drag destination
// Env (batch): DOM_STEPS='[{"ref":"e1","action":"fill","value":"a"},{"ref":"e4","action":"click"}]'
//   (step keys: ref, selector, action, value, key, toRef, toSelector); stops at the first failed step.
// Shared:
//   DOM_WAIT_TEXT text that must appear after the step(s) ('|' = any of); DOM_WAIT_MS (8000)
//   DOM_INPUT     trusted (default: real CDP mouse/keyboard, isTrusted=true) | js (DOM calls)
//   DOM_SETTLE_MS wait before verifying (default 500)
//   DOM_DIALOG    dismiss (default) | accept, for alert/confirm/prompt opened by the action
//   DOM_DIFF=0    skip the after-action [NEW] ref diff
export function createDomOperations(options: Record<string, any> = {}) {
  const cfg = readDomConfig(options);
  const MODE = cfg.mode;
  const WAIT_MS = cfg.waitMs;
  const WAIT_TEXT = cfg.waitText;
  const ACCEPT_DIALOG = cfg.acceptDialog;
  const DIFF = cfg.diff;
  const STEPS = readSteps(options, WAIT_TEXT);
  const ctx = { cfg, log: line => console.log(line) };

  const REASONS = {
    'blocked-by-actionability': d =>
      `DOM_BLOCKED ${[!d.visible && 'not visible (hidden, zero-size or off-screen; re-snapshot: refs may have shifted)', d.disabled && 'disabled', d.covered && `covered by ${d.coveredBy}`, d.stable === false && 'still moving (animation)'].filter(Boolean).join('; ') || 'actionability check failed'}`,
    'not-fillable': d =>
      `DOM_NOT_FILLABLE ${d.role} is not an editable field; pick a textbox/searchbox/contenteditable ref`,
    'not-checkable': d =>
      `DOM_NOT_CHECKABLE ${d.role} is not a checkbox/radio/switch`,
    'not-selectable': d =>
      `DOM_NOT_SELECTABLE ${d.role} is not a native <select>; click the custom combobox, then click its option ref`,
    'option-not-found': (d, s) =>
      `DOM_OPTION_NOT_FOUND ${JSON.stringify(s.value)}; options: ${JSON.stringify(d.options ?? [])}`,
    'not-focused': () =>
      'DOM_NOT_FOCUSED click did not focus the field (overlay or focus trap); retry with DOM_INPUT=js',
    'not-file-input': d =>
      `DOM_NOT_FILE_INPUT ${d.role} is not <input type=file>; target the file input (often hidden next to the styled button)`,
    'no-files': () =>
      'DOM_NO_FILES upload needs DOM_VALUE=/abs/path[|/abs/path2]',
  };

  // Prints one step; returns false when the batch should stop.
  function reportStep(step, label, details, effects, prefix) {
    if (!details.found) {
      console.log(
        `[FINDING] ${prefix}DOM target not found target=${JSON.stringify(label)}${details.error ? ` error=${JSON.stringify(details.error)}` : ''}`
      );
      return false;
    }
    if (details.acted) {
      const how =
        MODE === 'trusted' &&
        !['select', 'focus', 'upload'].includes(step.action)
          ? 'trusted input'
          : 'DOM';
      const valuePart = ['fill', 'type', 'select', 'upload'].includes(
        step.action
      )
        ? ` value=${JSON.stringify(step.value)}`
        : step.action === 'press'
          ? ` key=${JSON.stringify(step.key)}`
          : step.action === 'drag'
            ? ` to=${JSON.stringify(step.toRef || step.toSelector)} (${details.dragKind ?? 'pointer'})`
            : '';
      console.log(
        `[ACTION] ${prefix}${details.operation} ${JSON.stringify(details.accessibleNameGuess)} (${details.role}) via ${how}${valuePart}`
      );
      console.log(
        `[CODE] ${prefix}locator=${JSON.stringify(details.path)} action=${step.action}${valuePart}`
      );
      const a = details.after ?? {};
      const mutations = Math.max(
        0,
        (a.mutations ?? 0) - (details.mutationsBefore ?? 0)
      );
      const parts = [];
      if (details.verified !== undefined)
        parts.push(details.verified ? 'ok' : 'MISMATCH');
      if (effects.navigatedTo) parts.push(`navigated=${effects.navigatedTo}`);
      else if (a.url && a.url !== details.location) parts.push(`url=${a.url}`);
      if (!effects.navigatedTo) parts.push(`mutations=${mutations}`);
      if (a.state && Object.keys(a.state).length)
        parts.push(`state=${JSON.stringify(a.state)}`);
      if (step.action === 'scroll') parts.push(`scrollY=${a.scrollY}`);
      if (a.focus) parts.push(`focus=${a.focus}`);
      if (details.reaimed) parts.push('reaimed (target moved during approach)');
      if (a.dialogs) parts.push(`dialogs=${a.dialogs}`);
      if (effects.popup) parts.push(`popup=${effects.popup}`);
      if (a.connected === false && !effects.navigatedTo)
        parts.push('target-removed');
      console.log(`[VERIFY] ${prefix}${parts.join(' ')}`);
      if (a.events)
        console.log(
          `[METRIC] ${prefix}USER_EVENTS count=${a.events.length} trusted=${a.events.filter(e => e.trusted).length}`
        );
      if (effects.dialog)
        console.log(
          `[FINDING] ${prefix}JS_DIALOG ${effects.dialog.type} ${JSON.stringify(effects.dialog.message)} ${ACCEPT_DIALOG ? 'accepted' : 'dismissed (DOM_DIALOG=accept to accept)'}`
        );
      if (details.verified === false)
        console.log(
          `[FINDING] ${prefix}VERIFY_MISMATCH expected=${JSON.stringify(details.expected)}${MODE === 'trusted' ? ' — retry with DOM_ACTION=type or DOM_INPUT=js' : ''}`
        );
      const noEffect =
        !effects.navigatedTo &&
        !effects.popup &&
        !effects.dialog &&
        mutations === 0 &&
        a.url === details.location;
      details.noEffect = noEffect;
      if (
        noEffect &&
        ['click', 'dblclick', 'press', 'drag'].includes(step.action)
      )
        console.log(
          `[FINDING] ${prefix}NO_VISIBLE_EFFECT nothing changed in the DOM or URL; the target may need another action, or the handler is async (DOM_WAIT_TEXT or DOM_SETTLE_MS)`
        );
      return details.verified !== false;
    }
    console.log(
      `[METRIC] ${prefix}DOM target=${JSON.stringify(label)} found=true visible=${details.visible} disabled=${details.disabled} stable=${details.stable} covered=${details.covered} canOperate=${details.canOperate}`
    );
    console.log(
      `[METRIC] ${prefix}DOM role=${JSON.stringify(details.role)} name=${JSON.stringify(details.accessibleNameGuess)} bbox=${JSON.stringify(details.bbox)}${details.before && Object.keys(details.before).length ? ` state=${JSON.stringify(details.before)}` : ''}`
    );
    if (details.coveredBy)
      console.log(
        `[FINDING] ${prefix}DOM element is covered by ${details.coveredBy}`
      );
    if (details.operation?.startsWith('already-')) {
      console.log(
        `[ACTION] ${prefix}${details.operation} ${JSON.stringify(details.accessibleNameGuess)} (no change needed)`
      );
      return true;
    }
    const reason = REASONS[details.operation];
    if (reason) console.log(`[FINDING] ${prefix}${reason(details, step)}`);
    return step.action === 'inspect';
  }

  const currentRefs = async cdp =>
    (await collectRefs(cdp, {}).catch(() => ({ useful: [] }))).useful;

  function reportNewRefs(cdp, before, after) {
    const diff = recordNewRefs(cdp, before, after);
    if (!diff) return 0;
    console.log(`[DIFF] new=${diff.added} gone=${diff.gone}`);
    for (const line of diff.lines.slice(0, 20)) console.log(line);
    if (diff.lines.length > 20)
      console.log(`[NEW] … ${diff.lines.length - 20} more (run page-snapshot)`);
    return diff.added;
  }

  async function run(cdp) {
    await cdp.send('Runtime.enable');
    await cdp.send('DOM.enable');
    await cdp.send('Accessibility.enable');
    await cdp.send('Page.enable');

    if (STEPS.some(step => !['inspect', 'wait'].includes(step.action)))
      await (cdp.foreground ? cdp.foreground() : cdp.send('Page.bringToFront'));
    const ready = await waitForPageReady(cdp);
    if (!ready)
      console.log(
        "[FINDING] PAGE_NOT_FULLY_LOADED document did not reach an interactive state — a not-found/blocked result below may reflect a page that hasn't rendered yet, not a real absence"
      );

    // Effects the action may cause outside the element.
    const effects = { navigatedTo: null, dialog: null, popup: null };
    let navigatedAny = false;
    const onNav = ({ frame }) => {
      if (!frame.parentId) {
        effects.navigatedTo = frame.url;
        navigatedAny = true;
      }
    };
    const onDialog = async ({ type, message }) => {
      effects.dialog = {
        type,
        message: String(message),
        accepted: ACCEPT_DIALOG,
      };
      await cdp
        .send('Page.handleJavaScriptDialog', { accept: ACCEPT_DIALOG })
        .catch(() => {});
    };
    const onPopup = ({ url }) => {
      effects.popup = url;
    };
    cdp.on('Page.frameNavigated', onNav);
    cdp.on('Page.javascriptDialogOpening', onDialog);
    cdp.on('Page.windowOpen', onPopup);

    const mutating = STEPS.some(s => !['inspect', 'wait'].includes(s.action));
    const before = DIFF && mutating ? await currentRefs(cdp) : null;
    const results = [];
    let ok = true;
    for (const step of STEPS) {
      const prefix = STEPS.length > 1 ? `#${step.index} ` : '';
      Object.assign(effects, { navigatedTo: null, dialog: null, popup: null });
      if (step.action === 'wait') {
        const waited = step.waitTarget
          ? await locateWhenReady(cdp, step, ctx)
          : null;
        const { hit, ms } = waited
          ? {
              hit:
                waited.details.found &&
                waited.details.visible &&
                waited.details.waitTextMatches &&
                !waited.details.ambiguous
                  ? waited.label
                  : null,
              ms: null,
            }
          : await waitForText(cdp, step.value || WAIT_TEXT, ctx);
        console.log(
          hit
            ? `[WAIT] ${prefix}found ${JSON.stringify(hit)} after ${ms}ms`
            : `[FINDING] ${prefix}WAIT_TIMEOUT none of ${JSON.stringify(step.value || WAIT_TEXT)} appeared within ${WAIT_MS}ms`
        );
        results.push({ step, found: Boolean(hit), ms });
        if (!hit) {
          ok = false;
          break;
        }
        continue;
      }
      const { label, details, objectId } = await runStep(
        cdp,
        step,
        effects,
        ctx
      );
      results.push({ step, label, details, objectId });
      if (!reportStep(step, label, details, effects, prefix)) {
        ok = false;
        break;
      }
    }
    if (ok && WAIT_TEXT && STEPS.at(-1).action !== 'wait') {
      const { hit, ms } = await waitForText(
        cdp,
        WAIT_TEXT,
        ctx,
        results.at(-1)?.objectId
      );
      console.log(
        hit
          ? `[WAIT] found ${JSON.stringify(hit)} after ${ms}ms`
          : `[FINDING] WAIT_TIMEOUT none of ${JSON.stringify(WAIT_TEXT)} appeared within ${WAIT_MS}ms`
      );
      const last = results.at(-1);
      if (last?.details)
        last.details.postActionWait = {
          text: WAIT_TEXT,
          found: Boolean(hit),
          ms,
        };
      else if (last)
        last.postActionWait = { text: WAIT_TEXT, found: Boolean(hit), ms };
      if (!hit) ok = false;
    }
    const added =
      before && !navigatedAny
        ? reportNewRefs(cdp, before, await currentRefs(cdp))
        : 0;
    const lastHover =
      results.at(-1)?.step.action === 'hover' ? results.at(-1).details : null;
    if (lastHover?.noEffect && !added)
      console.log(
        '[FINDING] NO_VISIBLE_EFFECT hover changed nothing and revealed no refs; the menu may open on click'
      );
    if (before && navigatedAny)
      console.log('[NEXT] page navigated: run page-snapshot for fresh refs');
    if (STEPS.length > 1)
      console.log(
        `[METRIC] STEPS done=${results.filter(r => r.found || r.details?.acted || r.step.action === 'inspect').length}/${STEPS.length}${ok ? '' : ' stopped'}`
      );

    cdp.off('Page.frameNavigated', onNav);
    cdp.off('Page.javascriptDialogOpening', onDialog);
    cdp.off('Page.windowOpen', onPopup);

    const artifactPath = join(
      cdp.outputDir,
      options.artifactName || 'dom-check.json'
    );
    const artifact =
      STEPS.length === 1
        ? (results[0]?.details ?? results[0])
        : results.map(r => ({
            step: r.step,
            ...(r.details ?? { found: r.found, ms: r.ms }),
          }));
    writeFileSync(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`, {
      mode: 0o600,
    });
    cdp.upsertResourceMap?.('dom-operation-check', {
      type: 'dom-operation-check',
      target: results[0]?.label ?? null,
      action: STEPS.map(s => s.action).join(','),
      artifactPath,
      targetUrl: cdp.targetInfo.url,
    });
    console.log(`[ARTIFACT] DOM_CHECK ${artifactPath}`);
    if (!ok) process.exitCode = 1;
    return {
      ok,
      artifactPath,
      results: results.map(({ objectId, ...result }) => result),
    };
  }
  return { run };
}

export async function run(cdp, options) {
  return createDomOperations(options).run(cdp);
}
