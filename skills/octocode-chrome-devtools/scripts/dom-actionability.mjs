// Shared browser-side actionability checks, interpolated into Runtime.evaluate
// expressions across the DOM-inspection examples. One definition, not three.
export const ACTIONABILITY_HELPERS_JS = `
  function isVisible(el, rect, style) {
    const r = rect ?? el.getBoundingClientRect();
    const st = style ?? getComputedStyle(el);
    return Boolean(r.width && r.height && st.display !== 'none' && st.visibility !== 'hidden' && Number(st.opacity || '1') > 0);
  }
  function isDisabled(el) {
    return Boolean(el.matches(':disabled') || el.getAttribute('aria-disabled') === 'true' || el.closest('[inert]'));
  }
`;

// Node-side (not page-side) readiness gate: a fresh headless launch commits an
// internal about:blank document before the requested --url navigation lands,
// even though the CDP target list already reports the destination URL and
// document.readyState on that blank page already reads "complete". Any script
// that resolves a selector/ref immediately after launch can silently see the
// blank page and report a real element as not-found. Call this before
// resolving elements; it does not guarantee the destination page has finished
// its OWN async rendering, only that navigation past about:blank landed.
export async function waitForPageReady(cdp, timeoutMs = 8000) {
  await cdp.send('Page.enable');
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const { result } = await cdp.send('Runtime.evaluate', {
      expression: 'JSON.stringify({ready: document.readyState, blank: document.URL === "about:blank"})',
      returnByValue: true,
    });
    const state = JSON.parse(result.value);
    if (state.ready === 'complete' && !state.blank) return true;
    await new Promise((r) => setTimeout(r, 150));
  }
  return false;
}
