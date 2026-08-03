import { writeFileSync, readFileSync } from 'fs';
import { join, resolve } from 'path';
import { pathToFileURL } from 'url';

const { ACTIONABILITY_HELPERS_JS, waitForPageReady } = await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'dom-actionability.mjs')).href);

const SELECTOR = process.env.DOM_SELECTOR ?? 'button, [role="button"], input, textarea, select, a[href]';
const REF = process.env.DOM_REF ?? '';
const ACTION = process.env.DOM_ACTION ?? 'inspect';
const VALUE = process.env.DOM_VALUE ?? '';
const STABILITY_MS = Number.parseInt(process.env.DOM_STABILITY_MS ?? '150', 10);

function assertAllowedAction(action) {
  if (!['inspect', 'click', 'fill'].includes(action)) {
    throw new Error(`Unsupported DOM_ACTION=${action}. Use inspect, click, or fill.`);
  }
}

// Shared body: everything from "act on this element" onward, usable whether
// the element came from a CSS selector or a resolved snapshot ref (DOM_REF).
const CORE_BODY_JS = `
  ${ACTIONABILITY_HELPERS_JS}
  const cssEscape = globalThis.CSS?.escape ?? ((value) => String(value).replace(/[^a-zA-Z0-9_-]/g, '\\$&'));
  function shortText(value) {
    return String(value ?? '').replace(/\\s+/g, ' ').trim().slice(0, 160);
  }
  function elementPath(element) {
    const parts = [];
    let node = element;
    while (node && node.nodeType === Node.ELEMENT_NODE && parts.length < 8) {
      const tag = node.localName;
      const id = node.id ? '#' + cssEscape(node.id) : '';
      const testId = node.getAttribute('data-testid') ? '[data-testid="' + node.getAttribute('data-testid').replace(/"/g, '\\\\"') + '"]' : '';
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
  function accessibleNameGuess(element) {
    const labelledBy = element.getAttribute('aria-labelledby');
    if (labelledBy) {
      const text = labelledBy.split(/\\s+/).map(id => document.getElementById(id)?.textContent ?? '').join(' ');
      if (shortText(text)) return shortText(text);
    }
    const aria = element.getAttribute('aria-label');
    if (aria) return shortText(aria);
    if (element.labels?.length) return shortText([...element.labels].map(label => label.textContent).join(' '));
    if (element.alt) return shortText(element.alt);
    if (element.title) return shortText(element.title);
    return shortText(element.innerText || element.textContent || element.value);
  }
  async function stableRect(element, stabilityMs) {
    const first = element.getBoundingClientRect();
    await new Promise(resolve => setTimeout(resolve, stabilityMs));
    const second = element.getBoundingClientRect();
    const delta = Math.abs(first.x - second.x) + Math.abs(first.y - second.y) + Math.abs(first.width - second.width) + Math.abs(first.height - second.height);
    return { stable: delta < 1, first, second };
  }
  async function checkElement(element, action, fillValue, stabilityMs) {
    element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const style = getComputedStyle(element);
    const rectCheck = await stableRect(element, stabilityMs);
    const rect = rectCheck.second;
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const hit = document.elementFromPoint(cx, cy);
    const coveredBy = hit && hit !== element && !element.contains(hit) ? elementPath(hit) : null;
    const details = {
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
      disabled: isDisabled(element),
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
        // React (and similar) override the instance's own value setter to
        // track programmatic vs "real" changes; setting it directly leaves
        // that tracker seeing no change, so the app's onChange never fires
        // even though the DOM value visibly updates. Call the prototype's
        // native setter instead so the subsequent input event reads as a
        // real external change. Harmless on plain (non-framework) inputs —
        // it's the same underlying setter there.
        const proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype
          : element instanceof HTMLInputElement ? HTMLInputElement.prototype
          : null;
        const nativeSetter = proto ? Object.getOwnPropertyDescriptor(proto, 'value')?.set : null;
        if (nativeSetter) nativeSetter.call(element, fillValue);
        else element.value = fillValue;
        element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: fillValue }));
        element.dispatchEvent(new Event('change', { bubbles: true }));
        details.operation = 'filled';
      }
    } else {
      details.operation = action === 'inspect' ? 'inspected' : 'blocked-by-actionability';
    }
    return details;
  }
`;

// DOM_REF looks up the latest page-snapshot.json (written by page-snapshot.mjs)
// via the session's resource map, so callers don't have to pass a file path.
function resolveRefBackendNodeId(cdp, ref) {
  let resourceMap;
  try {
    resourceMap = JSON.parse(readFileSync(cdp.resourcesFile, 'utf8'));
  } catch {
    throw new Error('No page-snapshot resource found for this session — run examples/page-snapshot.mjs on the same --port first.');
  }
  const snapshotPath = resourceMap.resources?.['page-snapshot']?.artifactPath;
  if (!snapshotPath) throw new Error('No page-snapshot artifact recorded — run examples/page-snapshot.mjs on the same --port first.');
  const snapshot = JSON.parse(readFileSync(snapshotPath, 'utf8'));
  const entry = snapshot.refs?.[ref];
  if (!entry) throw new Error(`Ref ${ref} not found in ${snapshotPath}. Available: ${Object.keys(snapshot.refs ?? {}).join(', ')}`);
  return entry.backendDOMNodeId;
}

export async function run(cdp) {
  assertAllowedAction(ACTION);
  await cdp.send('Runtime.enable');
  await cdp.send('DOM.enable');
  await cdp.send('Accessibility.enable');

  // A fresh launch/navigation can still be on Chrome's internal about:blank
  // when this runs; querySelector against it silently reports a real element
  // as not-found instead of erroring. Wait past that before resolving anything.
  const ready = await waitForPageReady(cdp);
  if (!ready) console.log('[FINDING] PAGE_NOT_FULLY_LOADED document.readyState never reached "complete" — a not-found/blocked result below may reflect a page that hasn\'t rendered yet, not a real absence');

  let result;
  let targetLabel;

  if (REF) {
    targetLabel = `ref:${REF}`;
    const backendNodeId = resolveRefBackendNodeId(cdp, REF);
    const { object } = await cdp.send('DOM.resolveNode', { backendNodeId });
    if (!object?.objectId) {
      result = { result: { value: { ref: REF, found: false, action: ACTION, error: 'stale ref — page changed since the snapshot, take a new one' } } };
    } else {
      result = await cdp.send('Runtime.callFunctionOn', {
        objectId: object.objectId,
        awaitPromise: true,
        returnByValue: true,
        functionDeclaration: `async function() {
          ${CORE_BODY_JS}
          const element = this;
          const details = await checkElement(element, ${JSON.stringify(ACTION)}, ${JSON.stringify(VALUE)}, ${JSON.stringify(STABILITY_MS)});
          details.ref = ${JSON.stringify(REF)};
          return details;
        }`,
      });
    }
  } else {
    targetLabel = `selector:${SELECTOR}`;
    result = await cdp.send('Runtime.evaluate', {
      awaitPromise: true,
      returnByValue: true,
      expression: `(async () => {
        ${CORE_BODY_JS}
        const element = document.querySelector(${JSON.stringify(SELECTOR)});
        if (!element) return { selector: ${JSON.stringify(SELECTOR)}, found: false, action: ${JSON.stringify(ACTION)}, location: location.href };
        const details = await checkElement(element, ${JSON.stringify(ACTION)}, ${JSON.stringify(VALUE)}, ${JSON.stringify(STABILITY_MS)});
        details.selector = ${JSON.stringify(SELECTOR)};
        return details;
      })()`,
    });
  }

  const details = result.result?.value ?? { found: false, action: ACTION, error: 'No value returned' };

  const artifactPath = join(cdp.outputDir, 'dom-check.json');
  writeFileSync(artifactPath, `${JSON.stringify(details, null, 2)}\n`, { mode: 0o600 });
  cdp.upsertResourceMap?.('dom-operation-check', {
    type: 'dom-operation-check',
    target: targetLabel,
    action: ACTION,
    artifactPath,
    targetUrl: cdp.targetInfo.url,
  });

  const succeeded = details.operation === 'clicked' || details.operation === 'filled';
  if (!details.found) {
    console.log(`[FINDING] DOM target not found target=${JSON.stringify(targetLabel)}${details.error ? ` error=${JSON.stringify(details.error)}` : ''}`);
  } else if (succeeded) {
    // Actionability booleans are all implicitly true on success — full detail
    // (bbox, style, etc.) stays in the JSON artifact; only failure/blocked
    // paths print it inline, since that's where an agent needs it to recover.
    console.log(`[ACTION] ${details.operation} ${JSON.stringify(details.accessibleNameGuess)} (${details.role}) after actionability checks`);
    const valuePart = details.operation === 'filled' ? ` value=${JSON.stringify(VALUE)}` : '';
    console.log(`[CODE] locator=${JSON.stringify(details.path)} action=${ACTION}${valuePart}`);
  } else {
    console.log(`[METRIC] DOM target=${JSON.stringify(targetLabel)} found=true visible=${details.visible} disabled=${details.disabled} stable=${details.stable} covered=${details.covered} canOperate=${details.canOperate}`);
    console.log(`[METRIC] DOM role=${JSON.stringify(details.role)} name=${JSON.stringify(details.accessibleNameGuess)} bbox=${JSON.stringify(details.bbox)}`);
    if (details.coveredBy) console.log(`[FINDING] DOM element is covered by ${details.coveredBy}`);
    if (details.operation === 'blocked-by-actionability') console.log('[FINDING] DOM action blocked by actionability checks; inspect artifact for exact reason.');
  }
  console.log(`[ARTIFACT] DOM_CHECK ${artifactPath}`);
}
