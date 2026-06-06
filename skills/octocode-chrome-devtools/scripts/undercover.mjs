export async function applyStealthPatches(cdp, opts = {}) {
  const ua = opts.userAgent ??
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36';

  await cdp.send('Network.setUserAgentOverride', {
    userAgent: ua,
    platform: 'Win32',
    userAgentMetadata: {
      brands: [
        { brand: 'Chromium',      version: '124' },
        { brand: 'Google Chrome', version: '124' },
        { brand: 'Not-A.Brand',   version: '99'  },
      ],
      fullVersion: '124.0.0.0',
      platform: 'Windows',
      platformVersion: '10.0.0',
      architecture: 'x86',
      model: '',
      mobile: false,
    },
  });

  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width: 1920, height: 1080,
    deviceScaleFactor: 1,
    mobile: false,
    screenWidth: 1920, screenHeight: 1080,
    positionX: 0, positionY: 0,
  });

  await cdp.send('Emulation.setTimezoneOverride', { timezoneId: opts.timezone ?? 'America/New_York' });
  await cdp.send('Emulation.setLocaleOverride',   { locale:     opts.locale   ?? 'en-US' });

  await cdp.send('Emulation.setGeolocationOverride', {
    latitude:  opts.lat ?? 40.7128,
    longitude: opts.lon ?? -74.0060,
    accuracy: 100,
  });

  await cdp.send('Browser.grantPermissions', {
    permissions: ['geolocation', 'notifications', 'camera', 'microphone'],
    origin: opts.origin ?? undefined,
  });

  await cdp.send('Network.setExtraHTTPHeaders', {
    headers: {
      'Accept-Language':    'en-US,en;q=0.9',
      'Accept-Encoding':    'gzip, deflate, br',
      'sec-ch-ua':          '"Chromium";v="124", "Google Chrome";v="124", "Not-A.Brand";v="99"',
      'sec-ch-ua-mobile':   '?0',
      'sec-ch-ua-platform': '"Windows"',
    },
  });

  await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: `(function(){
    const _patchedFns = new WeakSet();
    const _nativeToString = Function.prototype.toString;
    Function.prototype.toString = new Proxy(_nativeToString, {
      apply(t, obj, a) { return _patchedFns.has(obj) ? 'function () { [native code] }' : Reflect.apply(t, obj, a); },
    });
    function def(obj, prop, fn) {
      _patchedFns.add(fn);
      try { Object.defineProperty(obj, prop, { get: fn, configurable: true, enumerable: true }); } catch (_) {}
    }

    def(navigator, 'webdriver',           () => undefined);
    def(navigator, 'vendor',              () => 'Google Inc.');
    def(navigator, 'platform',            () => 'Win32');
    def(navigator, 'maxTouchPoints',      () => 0);
    def(navigator, 'hardwareConcurrency', () => 8);
    def(navigator, 'deviceMemory',        () => 8);
    def(navigator, 'cookieEnabled',       () => true);
    def(navigator, 'languages',           () => ['en-US', 'en']);

    if (!window.chrome) window.chrome = {
      runtime: { id: undefined, connect: () => {}, sendMessage: () => {}, onMessage: { addListener: () => {}, removeListener: () => {} } },
      app: { isInstalled: false }, csi: () => {}, loadTimes: () => ({}),
    };

    def(navigator, 'plugins', () => Object.assign(
      [{ name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format', length: 1 },
       { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai', description: '', length: 1 },
       { name: 'Native Client',     filename: 'internal-nacl-plugin',  description: '', length: 2 }],
      { namedItem: () => null, refresh: () => {}, item: () => null }
    ));
    def(navigator, 'mimeTypes', () => Object.assign(
      [{ type: 'application/pdf', suffixes: 'pdf', description: '', enabledPlugin: null }],
      { namedItem: () => null, item: () => null }
    ));

    const _origQuery = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = function(p) {
      if (['notifications','camera','microphone'].includes(p.name)) return Promise.resolve({ state: 'prompt', onchange: null });
      return _origQuery(p);
    };
    _patchedFns.add(navigator.permissions.query);

    if (navigator.connection) {
      def(navigator.connection, 'rtt',          () => 50);
      def(navigator.connection, 'downlink',     () => 10);
      def(navigator.connection, 'effectiveType',() => '4g');
      def(navigator.connection, 'saveData',     () => false);
    }

    document.hasFocus = function() { return true; };
    _patchedFns.add(document.hasFocus);

    def(window, 'outerWidth',  () => 1920); def(window, 'outerHeight', () => 1080);
    def(window, 'screenX',     () => 20);   def(window, 'screenY',     () => 40);
    def(screen, 'width',       () => 1920); def(screen, 'height',      () => 1080);
    def(screen, 'availWidth',  () => 1920); def(screen, 'availHeight', () => 1040);
    def(screen, 'colorDepth',  () => 24);   def(screen, 'pixelDepth',  () => 24);

    const patchWebGL = ctx => {
      const o = ctx.prototype.getParameter;
      ctx.prototype.getParameter = function(p) {
        if (p === 37445) return 'Intel Inc.';
        if (p === 37446) return 'Intel Iris OpenGL Engine';
        return o.call(this, p);
      };
      _patchedFns.add(ctx.prototype.getParameter);
    };
    patchWebGL(WebGLRenderingContext);
    if (typeof WebGL2RenderingContext !== 'undefined') patchWebGL(WebGL2RenderingContext);

    const _origToDU = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function(...a) {
      const c = this.getContext('2d');
      if (c && this.width > 0 && this.height > 0) { const i = c.getImageData(0,0,1,1); i.data[0]^=1; c.putImageData(i,0,0); }
      return _origToDU.apply(this, a);
    };
    _patchedFns.add(HTMLCanvasElement.prototype.toDataURL);

    if (window.AudioBuffer) {
      const _origGCD = AudioBuffer.prototype.getChannelData;
      AudioBuffer.prototype.getChannelData = function(ch) {
        const d = _origGCD.call(this, ch);
        if (d.length > 0) d[0] += 1e-7 * Math.random();
        return d;
      };
      _patchedFns.add(AudioBuffer.prototype.getChannelData);
    }

    if (navigator.mediaDevices?.enumerateDevices) {
      const _origEnum = navigator.mediaDevices.enumerateDevices.bind(navigator.mediaDevices);
      navigator.mediaDevices.enumerateDevices = function() {
        return _origEnum().then(r => r.length > 0 ? r : [
          { deviceId: 'default', kind: 'audioinput',  label: '', groupId: 'default' },
          { deviceId: 'default', kind: 'audiooutput', label: '', groupId: 'default' },
          { deviceId: 'default', kind: 'videoinput',  label: '', groupId: 'default' },
        ]);
      };
      _patchedFns.add(navigator.mediaDevices.enumerateDevices);
    }

    const _ifd = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'contentWindow');
    if (_ifd?.get) {
      const _og = _ifd.get;
      Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
        get: function() {
          const w = _og.call(this);
          if (w) { try { Object.defineProperty(w.navigator, 'webdriver', { get: () => undefined }); } catch(_){} }
          return w;
        },
        configurable: true,
      });
    }
  })();` });

  console.log('[INJECT] Stealth patches applied (25 techniques)');
}

export async function verifyStealth(cdp) {
  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      webdriver:           navigator.webdriver,
      vendor:              navigator.vendor,
      platform:            navigator.platform,
      pluginCount:         navigator.plugins.length,
      hardwareConcurrency: navigator.hardwareConcurrency,
      deviceMemory:        navigator.deviceMemory,
      lang0:               navigator.languages?.[0],
      outerWidth:          window.outerWidth,
      screenWidth:         screen.width,
      hasFocus:            document.hasFocus(),
      chromeRuntime:       typeof window.chrome?.runtime,
      webglVendor: (() => {
        try {
          const c = document.createElement('canvas');
          const g = c.getContext('webgl');
          const e = g?.getExtension('WEBGL_debug_renderer_info');
          return e ? g.getParameter(e.UNMASKED_VENDOR_WEBGL) : null;
        } catch(_) { return null; }
      })(),
      toStringSpoof: Function.prototype.toString.call(navigator.__defineGetter__),
    })`,
    returnByValue: true,
  });

  const v = JSON.parse(result.value ?? '{}');
  const checks = [
    ['navigator.webdriver',    v.webdriver === undefined,       `expected undefined, got ${v.webdriver}`],
    ['navigator.vendor',       v.vendor    === 'Google Inc.',   `expected "Google Inc.", got "${v.vendor}"`],
    ['navigator.platform',     v.platform  === 'Win32',         `expected "Win32", got "${v.platform}"`],
    ['navigator.plugins >= 3', v.pluginCount >= 3,              `expected >=3, got ${v.pluginCount}`],
    ['hardwareConcurrency',    v.hardwareConcurrency === 8,     `expected 8, got ${v.hardwareConcurrency}`],
    ['deviceMemory',           v.deviceMemory        === 8,     `expected 8, got ${v.deviceMemory}`],
    ['languages[0]',           v.lang0 === 'en-US',             `expected "en-US", got "${v.lang0}"`],
    ['outerWidth',             v.outerWidth  === 1920,          `expected 1920, got ${v.outerWidth}`],
    ['screen.width',           v.screenWidth === 1920,          `expected 1920, got ${v.screenWidth}`],
    ['document.hasFocus()',    v.hasFocus === true,             `expected true, got ${v.hasFocus}`],
    ['window.chrome.runtime',  v.chromeRuntime === 'object',    `expected object, got ${v.chromeRuntime}`],
    ['WebGL vendor',           v.webglVendor === 'Intel Inc.',  `expected "Intel Inc.", got "${v.webglVendor}"`],
    ['toString native spoof',  (v.toStringSpoof ?? '').includes('[native code]'), `toString not spoofed`],
  ];

  let passed = 0, failed = 0;
  for (const [name, ok, msg] of checks) {
    if (ok) { console.log(`[INJECT] PASS: ${name}`); passed++; }
    else     { console.log(`[FINDING] STEALTH_FAIL: ${name} - ${msg}`); failed++; }
  }
  console.log(`[INJECT] Stealth self-test: ${passed}/${checks.length} passed${failed > 0 ? ` - ${failed} FAILED` : ' - all clear'}`);
  return { passed, failed, total: checks.length };
}
