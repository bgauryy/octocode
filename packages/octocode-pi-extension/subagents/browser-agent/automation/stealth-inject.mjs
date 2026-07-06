/**
 * stealth-inject.mjs
 *
 * JavaScript evasions to inject via CDP Page.addScriptToEvaluateOnNewDocument.
 * Runs before ANY page script — patches all common bot-detection signals.
 *
 * Usage in chromeDebug:
 *   scheme:"inject" scriptSource:<contents of STEALTH_SCRIPT below>
 *
 * Or via raw CDP:
 *   scheme:"raw" method:"Page.addScriptToEvaluateOnNewDocument" params:{source: STEALTH_SCRIPT}

 */

/**
 * The stealth script — paste into Page.addScriptToEvaluateOnNewDocument source param.
 * Self-contained, no external deps, works in any V8 context.
 */
export const STEALTH_SCRIPT = `(function() {
  'use strict';

  // ── 1. navigator.webdriver ──────────────────────────────────────────────────
  // CDP attachment sets this to true. Delete it so sites can't detect CDP.
  try {
    Object.defineProperty(navigator, 'webdriver', {
      get: () => undefined,
      configurable: true,
    });
  } catch(e) {}

  // ── 2. window.chrome ───────────────────────────────────────────────────────
  // Real Chrome has window.chrome with runtime, loadTimes, etc.
  // Headless Chrome is missing parts of this object.
  if (!window.chrome) {
    window.chrome = { runtime: {} };
  }
  if (!window.chrome.runtime) {
    window.chrome.runtime = {};
  }
  // Add loadTimes (used by fingerprinters to detect headless)
  if (!window.chrome.loadTimes) {
    window.chrome.loadTimes = function() {
      return {
        requestTime: Date.now() / 1000 - Math.random() * 2,
        startLoadTime: Date.now() / 1000 - Math.random(),
        commitLoadTime: Date.now() / 1000 - Math.random() * 0.5,
        finishDocumentLoadTime: 0,
        finishLoadTime: 0,
        firstPaintTime: 0,
        firstPaintAfterLoadTime: 0,
        navigationType: 'Other',
        wasFetchedViaSpdy: false,
        wasNpnNegotiated: false,
        npnNegotiatedProtocol: 'unknown',
        wasAlternateProtocolAvailable: false,
        connectionInfo: 'http/1.1',
      };
    };
  }
  if (!window.chrome.csi) {
    window.chrome.csi = function() {
      return {
        onloadT: Date.now(),
        pageT: Math.random() * 3000 + 1000,
        startE: Date.now() - Math.random() * 5000,
        tran: 15,
      };
    };
  }

  // ── 3. navigator.plugins ───────────────────────────────────────────────────
  // Headless has 0 plugins. Real Chrome has 3+ (PDF viewer, etc.)
  if (navigator.plugins.length === 0) {
    const makePlugin = (name, filename, desc, mimeTypes) => {
      const plugin = Object.create(Plugin.prototype);
      Object.defineProperties(plugin, {
        name: { value: name, enumerable: true },
        filename: { value: filename, enumerable: true },
        description: { value: desc, enumerable: true },
        length: { value: mimeTypes.length, enumerable: true },
      });
      mimeTypes.forEach((mt, i) => {
        const mime = Object.create(MimeType.prototype);
        Object.defineProperties(mime, {
          type: { value: mt.type, enumerable: true },
          suffixes: { value: mt.suffixes, enumerable: true },
          description: { value: mt.desc, enumerable: true },
          enabledPlugin: { value: plugin, enumerable: true },
        });
        plugin[i] = mime;
      });
      return plugin;
    };

    const plugins = [
      makePlugin('Chrome PDF Plugin', 'internal-pdf-viewer', 'Portable Document Format',
        [{ type: 'application/x-google-chrome-pdf', suffixes: 'pdf', desc: 'Portable Document Format' }]),
      makePlugin('Chrome PDF Viewer', 'mhjfbmdgcfjbbpaeojofohoefgiehjai', '',
        [{ type: 'application/pdf', suffixes: 'pdf', desc: '' }]),
      makePlugin('Native Client', 'internal-nacl-plugin', '',
        [{ type: 'application/x-nacl', suffixes: '', desc: 'Native Client Executable' },
         { type: 'application/x-pnacl', suffixes: '', desc: 'Portable Native Client Executable' }]),
    ];

    try {
      Object.defineProperty(navigator, 'plugins', {
        get: () => {
          const arr = [...plugins];
          arr.item = (i) => plugins[i] || null;
          arr.namedItem = (name) => plugins.find(p => p.name === name) || null;
          arr.refresh = () => {};
          Object.defineProperty(arr, 'length', { value: plugins.length });
          return arr;
        },
        configurable: true,
      });
      Object.defineProperty(navigator, 'mimeTypes', {
        get: () => {
          const mimes = plugins.flatMap(p => Array.from({length: p.length}, (_, i) => p[i]));
          const arr = [...mimes];
          arr.item = (i) => mimes[i] || null;
          arr.namedItem = (type) => mimes.find(m => m.type === type) || null;
          Object.defineProperty(arr, 'length', { value: mimes.length });
          return arr;
        },
        configurable: true,
      });
    } catch(e) {}
  }

  // ── 4. navigator.languages ─────────────────────────────────────────────────
  try {
    Object.defineProperty(navigator, 'languages', {
      get: () => ['en-US', 'en'],
      configurable: true,
    });
  } catch(e) {}

  // ── 5. Permissions API ─────────────────────────────────────────────────────
  // Real Chrome returns 'prompt' for notifications; headless returns 'denied'
  const originalQuery = window.Permissions && window.Permissions.prototype.query;
  if (originalQuery) {
    window.Permissions.prototype.query = function(parameters) {
      return parameters.name === 'notifications'
        ? Promise.resolve({ state: Notification.permission, onchange: null })
        : originalQuery.call(this, parameters);
    };
  }

  // ── 6. Notification.permission ─────────────────────────────────────────────
  // Headless has 'denied'; real browsers start at 'default'
  try {
    if (Notification.permission === 'denied') {
      Object.defineProperty(Notification, 'permission', {
        get: () => 'default',
        configurable: true,
      });
    }
  } catch(e) {}

  // ── 7. console.debug ───────────────────────────────────────────────────────
  // Some detection tests check if console.debug === console.log (it shouldn't be)
  // In headless, these can be identical. Ensure they're distinct.
  if (typeof console !== 'undefined' && console.debug === console.log) {
    console.debug = console.log.bind(console);
  }

  // ── 8. User-Agent HeadlessChrome removal ───────────────────────────────────
  // Chrome --headless=new already removes this in Chrome 112+, but older headless kept it.
  // Safe to run even on new headless — only patches if present.
  try {
    const ua = navigator.userAgent;
    if (ua.includes('HeadlessChrome')) {
      const cleanUA = ua.replace('HeadlessChrome', 'Chrome');
      Object.defineProperty(navigator, 'userAgent', { get: () => cleanUA, configurable: true });
      Object.defineProperty(navigator, 'appVersion', {
        get: () => cleanUA.replace('Mozilla/', ''), configurable: true,
      });
    }
  } catch(e) {}

  // ── 9. WebGL vendor spoofing ───────────────────────────────────────────────
  // Headless: "Google SwiftShader" (software renderer) → bot tell.
  // Patch to return ANGLE (DirectX/Metal/Vulkan) — what real Chrome shows.
  try {
    const getParam = WebGLRenderingContext.prototype.getParameter;
    const ext_re = /UNMASKED_VENDOR_WEBGL|UNMASKED_RENDERER_WEBGL/;
    WebGLRenderingContext.prototype.getParameter = function(parameter) {
      if (parameter === 37445) return 'Intel Inc.';                         // UNMASKED_VENDOR
      if (parameter === 37446) return 'Intel Iris OpenGL Engine';           // UNMASKED_RENDERER
      return getParam.call(this, parameter);
    };
    const getParam2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(parameter) {
      if (parameter === 37445) return 'Intel Inc.';
      if (parameter === 37446) return 'Intel Iris OpenGL Engine';
      return getParam2.call(this, parameter);
    };
  } catch(e) {}

  // ── 10. Canvas fingerprint noise ───────────────────────────────────────────
  // Identical canvas output across sessions is a fingerprinting signal.
  // Add imperceptible noise to canvas toDataURL + getImageData.
  try {
    const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function(type, quality) {
      const ctx = this.getContext('2d');
      if (ctx) {
        const imgData = ctx.getImageData(0, 0, this.width || 1, this.height || 1);
        // Alter 1 LSB of 1 random pixel — invisible, but breaks hash equality
        const idx = Math.floor(Math.random() * imgData.data.length / 4) * 4;
        imgData.data[idx] = (imgData.data[idx] ^ 1) & 0xff;
        ctx.putImageData(imgData, 0, 0);
      }
      return origToDataURL.call(this, type, quality);
    };
  } catch(e) {}

  // ── 11. iframe contentWindow protection ────────────────────────────────────
  // Detection: create iframe, check if contentWindow.navigator !== window.navigator
  // Both should have patched webdriver = undefined.
  try {
    const origGetter = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'contentWindow');
    if (origGetter && origGetter.get) {
      Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
        get: function() {
          const win = origGetter.get.call(this);
          if (win && win.navigator) {
            try {
              Object.defineProperty(win.navigator, 'webdriver', {
                get: () => undefined, configurable: true,
              });
            } catch(e) {}
          }
          return win;
        },
        configurable: true,
      });
    }
  } catch(e) {}

  // ── 12. screen dimensions ──────────────────────────────────────────────────
  // Headless screen is often 0x0 or 800x600 — dead giveaway.
  // Patch to realistic 1920x1080 values.
  try {
    const screenProps = {
      width: 1920, height: 1080,
      availWidth: 1920, availHeight: 1040,
      colorDepth: 24, pixelDepth: 24,
    };
    for (const [prop, val] of Object.entries(screenProps)) {
      try {
        Object.defineProperty(screen, prop, { get: () => val, configurable: true });
      } catch(e) {}
    }
  } catch(e) {}

  // ── 13. outerWidth / outerHeight ───────────────────────────────────────────
  // Real browsers: outerWidth >= innerWidth (window chrome adds height/width).
  // Headless: outerWidth === 0 or outerWidth < innerWidth — detection signal.
  try {
    if (window.outerWidth === 0) {
      Object.defineProperty(window, 'outerWidth', { get: () => window.innerWidth, configurable: true });
    }
    if (window.outerHeight === 0) {
      Object.defineProperty(window, 'outerHeight', { get: () => window.innerHeight + 85, configurable: true });
    }
  } catch(e) {}

  // ── 14. navigator.vendor ───────────────────────────────────────────────────
  // Real Chrome reports "Google Inc." — headless may differ.
  try {
    Object.defineProperty(navigator, 'vendor', {
      get: () => 'Google Inc.',
      configurable: true,
    });
  } catch(e) {}

  // ── 15. navigator.hardwareConcurrency ──────────────────────────────────────
  // Headless often reports 1. Real machines report 4-16.
  try {
    if (!navigator.hardwareConcurrency || navigator.hardwareConcurrency < 4) {
      Object.defineProperty(navigator, 'hardwareConcurrency', {
        get: () => 4,
        configurable: true,
      });
    }
  } catch(e) {}

  // ── 16. chrome.app ─────────────────────────────────────────────────────────
  // Real Chrome has window.chrome.app with specific methods.
  try {
    if (window.chrome && !('app' in window.chrome)) {
      const APP_STATIC = {
        isInstalled: false,
        InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' },
        RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' },
      };
      window.chrome.app = {
        ...APP_STATIC,
        get isInstalled() { return false; },
        getDetails() { return null; },
        getIsInstalled() { return false; },
        runningState() { return 'cannot_run'; },
      };
    }
  } catch(e) {}

  // ── 17. media.codecs — canPlayType ─────────────────────────────────────────
  // Chromium (not Chrome) doesn't support proprietary codecs like H.264/AAC.
  // Headless Chromium returns '' for these — real Chrome returns 'probably'/'maybe'.
  try {
    const _canPlayType = HTMLMediaElement.prototype.canPlayType;
    HTMLMediaElement.prototype.canPlayType = function(type) {
      if (!type) return _canPlayType.apply(this, arguments);
      const t = type.trim();
      const mime = t.split(';')[0].trim();
      const codecMatch = t.match(/codecs="([^"]+)"/);
      const codecs = codecMatch ? codecMatch[1].split(',').map(c => c.trim()) : [];
      if (mime === 'video/mp4' && codecs.includes('avc1.42E01E')) return 'probably';
      if (mime === 'audio/x-m4a' && codecs.length === 0) return 'maybe';
      if (mime === 'audio/aac' && codecs.length === 0) return 'probably';
      return _canPlayType.apply(this, arguments);
    };
  } catch(e) {}

})();`;

/**
 * Get the stealth script as a compact single line (for CDP params).
 * Equivalent to the above but minified.
 */
export function getStealthScript() {
  return STEALTH_SCRIPT;
}

/**
 * Build the CDP params to pass to Page.addScriptToEvaluateOnNewDocument.
 */
export function getStealthInjectParams() {
  return { source: STEALTH_SCRIPT };
}
