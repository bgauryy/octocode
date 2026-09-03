/**
 * detection-check.mjs
 *
 * Self-test: evaluate all bot-detection signals in the current page context.
 * Returns a score and per-signal breakdown.
 *
 * Use via chromeDebug scheme:"raw" method:"Runtime.evaluate" after injecting
 * stealth-inject.mjs, or paste DETECTION_CHECK_SCRIPT as the expression directly.
 *
 * Usage:
 *   scheme:"raw" method:"Runtime.evaluate"
 *   params:{ expression: DETECTION_CHECK_SCRIPT, returnByValue: true, awaitPromise: true }
 */

export const DETECTION_CHECK_SCRIPT = `(async () => {
  const results = {};

  // 1. navigator.webdriver
  results.webdriver = navigator.webdriver;
  results.webdriver_ok = navigator.webdriver === undefined || navigator.webdriver === false;

  // 2. window.chrome
  results.chrome_exists = typeof window.chrome !== 'undefined';
  results.chrome_runtime = !!(window.chrome && window.chrome.runtime);
  results.chrome_app = !!(window.chrome && window.chrome.app);
  results.chrome_ok = results.chrome_exists && results.chrome_runtime;

  // 3. navigator.plugins
  results.plugins_count = navigator.plugins.length;
  results.plugins_ok = navigator.plugins.length >= 2;

  // 4. navigator.languages
  results.languages = [...(navigator.languages || [])];
  results.languages_ok = navigator.languages && navigator.languages.length > 0;

  // 5. navigator.vendor
  results.vendor = navigator.vendor;
  results.vendor_ok = navigator.vendor === 'Google Inc.';

  // 6. navigator.hardwareConcurrency
  results.hardwareConcurrency = navigator.hardwareConcurrency;
  results.hardwareConcurrency_ok = navigator.hardwareConcurrency >= 2;

  // 7. user agent
  results.userAgent = navigator.userAgent;
  results.headless_ua = navigator.userAgent.includes('HeadlessChrome');
  results.ua_ok = !results.headless_ua;

  // 8. screen
  results.screen_width = screen.width;
  results.screen_height = screen.height;
  results.screen_ok = screen.width >= 1024 && screen.height >= 768;

  // 9. outerWidth >= innerWidth
  results.outer_width = window.outerWidth;
  results.inner_width = window.innerWidth;
  results.dimensions_ok = window.outerWidth >= window.innerWidth;

  // 10. WebGL vendor
  try {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
    if (gl) {
      const ext = gl.getExtension('WEBGL_debug_renderer_info');
      if (ext) {
        results.webgl_vendor = gl.getParameter(ext.UNMASKED_VENDOR_WEBGL);
        results.webgl_renderer = gl.getParameter(ext.UNMASKED_RENDERER_WEBGL);
        results.webgl_ok = !results.webgl_renderer.includes('SwiftShader');
      } else {
        results.webgl_ok = true;
      }
    }
  } catch(e) {
    results.webgl_ok = true;
  }

  // 11. Permissions
  try {
    const perm = await navigator.permissions.query({ name: 'notifications' });
    results.permission_notifications = perm.state;
    results.permission_ok = perm.state !== 'denied';
  } catch(e) {
    results.permission_ok = true;
  }

  // 12. media.codecs
  try {
    const video = document.createElement('video');
    results.codec_mp4 = video.canPlayType('video/mp4; codecs="avc1.42E01E"');
    results.codec_aac = video.canPlayType('audio/aac');
    results.codecs_ok = results.codec_mp4 !== '' || results.codec_aac !== '';
  } catch(e) {
    results.codecs_ok = true;
  }

  // 13. iframe contentWindow.navigator.webdriver
  try {
    const iframe = document.createElement('iframe');
    iframe.style.display = 'none';
    document.body.appendChild(iframe);
    results.iframe_webdriver = iframe.contentWindow.navigator.webdriver;
    results.iframe_ok = iframe.contentWindow.navigator.webdriver === undefined;
    document.body.removeChild(iframe);
  } catch(e) {
    results.iframe_ok = true;
  }

  // 14. Object.prototype pollution check
  try {
    const pollution = Object.getOwnPropertyNames(Object.prototype).filter(k =>
      !['constructor','__defineGetter__','__defineSetter__','hasOwnProperty',
        '__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable',
        'toString','valueOf','__proto__','toLocaleString'].includes(k)
    );
    results.prototype_pollution = pollution;
    results.pollution_ok = pollution.length === 0;
  } catch(e) {
    results.pollution_ok = true;
  }

  // Score
  const checks = [
    results.webdriver_ok, results.chrome_ok, results.plugins_ok,
    results.languages_ok, results.vendor_ok, results.hardwareConcurrency_ok,
    results.ua_ok, results.screen_ok, results.dimensions_ok,
    results.webgl_ok, results.permission_ok, results.codecs_ok,
    results.iframe_ok, results.pollution_ok,
  ];
  results.score = checks.filter(Boolean).length;
  results.total = checks.length;
  results.pass_rate = (results.score / results.total * 100).toFixed(0) + '%';
  results.verdict = results.score === results.total ? 'CLEAN' :
                    results.score >= results.total * 0.8 ? 'MOSTLY_CLEAN' : 'DETECTED';

  return results;
})()`;

/**
 * Compact summary of detection results for [FINDING] emission.
 * Call after evaluating DETECTION_CHECK_SCRIPT.
 */
export function summarizeDetection(results) {
  const lines = [];
  lines.push(`[METRIC] stealth score: ${results.score}/${results.total} (${results.pass_rate}) — ${results.verdict}`);

  const checks = {
    'navigator.webdriver': results.webdriver_ok,
    'window.chrome': results.chrome_ok,
    'plugins': results.plugins_ok,
    'vendor': results.vendor_ok,
    'hardwareConcurrency': results.hardwareConcurrency_ok,
    'userAgent': results.ua_ok,
    'screen': results.screen_ok,
    'outerWidth': results.dimensions_ok,
    'webgl': results.webgl_ok,
    'permissions': results.permission_ok,
    'codecs': results.codecs_ok,
    'iframe.webdriver': results.iframe_ok,
    'prototype': results.pollution_ok,
  };

  for (const [name, ok] of Object.entries(checks)) {
    if (!ok) {
      lines.push(`[FINDING] DETECTION_SIGNAL: ${name} — bot indicator present`);
    }
  }

  if (results.verdict === 'CLEAN') {
    lines.push('[ACTION] No bot detection signals — stealth posture is good');
  } else {
    lines.push('[ACTION] Inject stealth script before navigation: scheme:"inject" or scheme:"raw" method:"Page.addScriptToEvaluateOnNewDocument"');
  }

  return lines;
}
