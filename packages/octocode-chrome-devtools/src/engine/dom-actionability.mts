// Shared browser-side actionability checks, interpolated into Runtime.evaluate
// expressions across the DOM-inspection checks.
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

// Wait past the initial about:blank document; SPA rendering may continue afterward.
export async function waitForPageReady(
  cdp,
  timeoutMs = 8000,
  { selector = '', text = '', state = 'interactive' } = {}
) {
  await cdp.send('Page.enable');
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    let evaluated;
    try {
      evaluated = await cdp.send('Runtime.evaluate', {
        expression: `(() => {
          const selector = ${JSON.stringify(selector)};
          const el = selector ? document.querySelector(selector) : null;
          const text = ${JSON.stringify(text)};
          return { ready: document.readyState, blank: document.URL === 'about:blank',
            content: (!selector || Boolean(el)) && (!text || document.body?.innerText?.includes(text)) };
        })()`,
        returnByValue: true,
      });
    } catch (error) {
      if (
        !/context.*destroyed|Cannot find context|Inspected target navigated/i.test(
          error.message
        )
      )
        throw error;
    }
    if (evaluated?.exceptionDetails)
      throw new Error(
        evaluated.exceptionDetails.exception?.description ||
          evaluated.exceptionDetails.text
      );
    const current = evaluated?.result?.value;
    if (
      current &&
      !current.blank &&
      current.content &&
      (state === 'complete'
        ? current.ready === 'complete'
        : ['interactive', 'complete'].includes(current.ready))
    )
      return true;
    await new Promise(r => setTimeout(r, 150));
  }
  return false;
}
