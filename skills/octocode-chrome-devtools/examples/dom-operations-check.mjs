import { writeFileSync } from 'fs';
import { join } from 'path';

const SELECTOR = process.env.DOM_SELECTOR ?? 'button, [role="button"], input, textarea, select, a[href]';
const ACTION = process.env.DOM_ACTION ?? 'inspect';
const VALUE = process.env.DOM_VALUE ?? '';
const STABILITY_MS = Number.parseInt(process.env.DOM_STABILITY_MS ?? '150', 10);

function assertAllowedAction(action) {
  if (!['inspect', 'click', 'fill'].includes(action)) {
    throw new Error(`Unsupported DOM_ACTION=${action}. Use inspect, click, or fill.`);
  }
}

export async function run(cdp) {
  assertAllowedAction(ACTION);
  await cdp.send('Runtime.enable');
  await cdp.send('DOM.enable');
  await cdp.send('Accessibility.enable');

  const result = await cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `(async () => {
      const selector = ${JSON.stringify(SELECTOR)};
      const action = ${JSON.stringify(ACTION)};
      const fillValue = ${JSON.stringify(VALUE)};
      const stabilityMs = ${JSON.stringify(STABILITY_MS)};

      const cssEscape = globalThis.CSS?.escape ?? ((value) => String(value).replace(/[^a-zA-Z0-9_-]/g, '\\$&'));
      function shortText(value) {
        return String(value ?? '').replace(/\s+/g, ' ').trim().slice(0, 160);
      }
      function elementPath(element) {
        const parts = [];
        let node = element;
        while (node && node.nodeType === Node.ELEMENT_NODE && parts.length < 8) {
          const tag = node.localName;
          const id = node.id ? '#' + cssEscape(node.id) : '';
          const testId = node.getAttribute('data-testid') ? '[data-testid="' + node.getAttribute('data-testid').replace(/"/g, '\\"') + '"]' : '';
          let nth = '';
          if (!id && !testId && node.parentElement) {
            const siblings = [...node.parentElement.children].filter(child => child.localName === tag);
            if (siblings.length > 1) nth = ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')';
          }
          parts.unshift(tag + id + testId + nth);
          const root = node.getRootNode();
          if (root instanceof ShadowRoot) {
            parts.unshift('::shadow');
            node = root.host;
          } else {
            node = node.parentElement;
          }
        }
        return parts.join(' > ');
      }
      function isVisible(element, rect, style) {
        return Boolean(rect.width && rect.height && style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity || '1') > 0);
      }
      function disabledState(element) {
        return Boolean(element.disabled || element.getAttribute('aria-disabled') === 'true' || element.closest('[inert]'));
      }
      function accessibleNameGuess(element) {
        const labelledBy = element.getAttribute('aria-labelledby');
        if (labelledBy) {
          const text = labelledBy.split(/\s+/).map(id => document.getElementById(id)?.textContent ?? '').join(' ');
          if (shortText(text)) return shortText(text);
        }
        const aria = element.getAttribute('aria-label');
        if (aria) return shortText(aria);
        if (element.labels?.length) return shortText([...element.labels].map(label => label.textContent).join(' '));
        if (element.alt) return shortText(element.alt);
        if (element.title) return shortText(element.title);
        return shortText(element.innerText || element.textContent || element.value);
      }
      async function stableRect(element) {
        const first = element.getBoundingClientRect();
        await new Promise(resolve => setTimeout(resolve, stabilityMs));
        const second = element.getBoundingClientRect();
        const delta = Math.abs(first.x - second.x) + Math.abs(first.y - second.y) + Math.abs(first.width - second.width) + Math.abs(first.height - second.height);
        return { stable: delta < 1, first, second };
      }

      const element = document.querySelector(selector);
      if (!element) {
        return { selector, found: false, action, location: location.href };
      }

      element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
      const style = getComputedStyle(element);
      const rectCheck = await stableRect(element);
      const rect = rectCheck.second;
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      const hit = document.elementFromPoint(cx, cy);
      const coveredBy = hit && hit !== element && !element.contains(hit) ? elementPath(hit) : null;
      const details = {
        selector,
        found: true,
        action,
        location: location.href,
        tag: element.localName,
        path: elementPath(element),
        id: element.id || null,
        name: element.getAttribute('name'),
        type: element.getAttribute('type'),
        role: element.getAttribute('role') || element.localName,
        accessibleNameGuess: accessibleNameGuess(element),
        text: shortText(element.innerText || element.textContent),
        visible: isVisible(element, rect, style),
        disabled: disabledState(element),
        stable: rectCheck.stable,
        covered: Boolean(coveredBy),
        coveredBy,
        bbox: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
        style: {
          display: style.display,
          visibility: style.visibility,
          opacity: style.opacity,
          pointerEvents: style.pointerEvents,
          position: style.position,
          zIndex: style.zIndex,
        },
        canOperate: false,
        operation: null,
      };
      details.canOperate = details.visible && !details.disabled && !details.covered && details.stable;

      if (action === 'click' && details.canOperate) {
        element.click();
        details.operation = 'clicked';
      } else if (action === 'fill' && details.canOperate) {
        if (!('value' in element)) {
          details.operation = 'not-fillable';
        } else {
          element.focus();
          element.value = fillValue;
          element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: fillValue }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
          details.operation = 'filled';
        }
      } else {
        details.operation = action === 'inspect' ? 'inspected' : 'blocked-by-actionability';
      }
      return details;
    })()`,
  });

  const details = result.result?.value ?? { selector: SELECTOR, found: false, action: ACTION, error: 'No Runtime.evaluate value returned' };

  let axSummary = null;
  try {
    const ax = await cdp.send('Accessibility.getFullAXTree', { depth: 4 });
    axSummary = {
      nodeCount: ax.nodes?.length ?? 0,
      focused: ax.nodes?.find(node => node.focused?.value === true)?.name?.value ?? null,
    };
    details.accessibilityTreeSummary = axSummary;
  } catch (error) {
    details.accessibilityTreeSummary = { error: error.message };
  }

  const artifactPath = join(cdp.outputDir, 'dom-check.json');
  writeFileSync(artifactPath, `${JSON.stringify(details, null, 2)}\n`, { mode: 0o600 });
  cdp.upsertResourceMap?.('dom-operation-check', {
    type: 'dom-operation-check',
    selector: SELECTOR,
    action: ACTION,
    artifactPath,
    targetUrl: cdp.targetInfo.url,
  });

  if (!details.found) {
    console.log(`[FINDING] DOM selector not found selector=${JSON.stringify(SELECTOR)}`);
  } else {
    console.log(`[METRIC] DOM selector=${JSON.stringify(SELECTOR)} found=true visible=${details.visible} disabled=${details.disabled} stable=${details.stable} covered=${details.covered} canOperate=${details.canOperate}`);
    console.log(`[METRIC] DOM role=${JSON.stringify(details.role)} name=${JSON.stringify(details.accessibleNameGuess)} bbox=${JSON.stringify(details.bbox)}`);
    if (details.coveredBy) console.log(`[FINDING] DOM element is covered by ${details.coveredBy}`);
    if (details.operation === 'blocked-by-actionability') console.log('[FINDING] DOM action blocked by actionability checks; inspect artifact for exact reason.');
    if (details.operation === 'clicked') console.log('[ACTION] clicked element after actionability checks');
    if (details.operation === 'filled') console.log('[ACTION] filled element and dispatched input/change events');
  }
  if (axSummary) console.log(`[METRIC] AX nodes=${axSummary.nodeCount} focused=${JSON.stringify(axSummary.focused)}`);
  console.log(`[ARTIFACT] DOM_CHECK ${artifactPath}`);
}
