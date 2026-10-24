/**
 * Opt-in emulation that removes headless tells with CDP overrides only.
 *
 * Every value stays consistent with the host browser: the User-Agent string, the
 * UA Client Hints, navigator.platform, WebGL, fonts and workers all describe the
 * same machine. No page JavaScript is injected, so there are no patched getters,
 * own navigator properties or non-native function sources for a page to detect,
 * and nothing outlives the CDP session that applied it.
 */
import type * as T from './stealth-session.mjs';
import { errorMessage } from './error-message.mjs';

// Pauses new child targets until their overrides are set, so their first request
// already carries them.
const STEALTH_AUTO_ATTACH: T.AutoAttachParams = {
  autoAttach: true,
  waitForDebuggerOnStart: true,
  flatten: true,
};

const CHILD_TYPES = new Set([
  'iframe',
  'worker',
  'shared_worker',
  'service_worker',
]);

// Headless Chrome reports an 800x600 screen inside a larger window; report a common
// desktop screen that still contains the window.
const DESKTOP_SCREEN = { width: 1920, height: 1080 };

const HOST_PROBE = `(async () => {
  if (!location.href.startsWith('chrome://') || !navigator.userAgentData) return null;
  const metadata = await navigator.userAgentData.getHighEntropyValues([
    'architecture', 'bitness', 'formFactors', 'fullVersionList', 'model',
    'platformVersion', 'wow64',
  ]);
  return { userAgent: navigator.userAgent, platform: navigator.platform, metadata };
})()`;

/**
 * Reads the browser's own identity from a fresh target. navigator.userAgentData
 * needs a secure context; chrome://version is one and carries no overrides.
 */
async function readHostIdentity(cdp: T.StealthSession) {
  const { targetId } = await cdp.send('Target.createTarget', {
    url: 'chrome://version/',
    background: true,
  });
  try {
    const { sessionId } = await cdp.send('Target.attachToTarget', {
      targetId,
      flatten: true,
    });
    const deadline = Date.now() + 5000;
    for (;;) {
      const { result, exceptionDetails } = await cdp.send(
        'Runtime.evaluate',
        { expression: HOST_PROBE, awaitPromise: true, returnByValue: true },
        sessionId
      );
      if (exceptionDetails)
        throw new Error(
          `Browser identity unavailable: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`
        );
      if (result?.value) return hostIdentity(result.value);
      if (Date.now() > deadline)
        throw new Error(
          'Browser identity unavailable: chrome://version did not load within 5000ms'
        );
      await new Promise(r => setTimeout(r, 100));
    }
  } finally {
    await cdp.send('Target.closeTarget', { targetId }).catch(() => {});
  }
}

// The OS a user agent string names, in the shape its client hints report. Null when it names none: nothing can then be matched to it.
function userAgentOs(userAgent: string): T.UserAgentOs | null {
  const os = (fields: Partial<T.UserAgentOs>): T.UserAgentOs => ({
    platform: '',
    mobile: false,
    platformVersion: '',
    architecture: '',
    bitness: '',
    model: '',
    ...fields,
  });
  const android = userAgent.match(/Android (\d+(?:\.\d+)*)(?:; ([^;)]+))?/);
  if (android)
    return os({
      platform: 'Android',
      mobile: /\bMobile\b/.test(userAgent),
      platformVersion: android[1],
      model: android[2]?.trim() ?? '',
    });
  const apple = userAgent.match(/\b(iPhone|iPad|iPod)\b/);
  if (apple)
    return os({
      platform: 'iOS',
      mobile: apple[1] !== 'iPad',
      platformVersion: (
        userAgent.match(/OS (\d+(?:_\d+)*) like Mac OS X/)?.[1] ?? ''
      ).replaceAll('_', '.'),
    });
  const windows = userAgent.match(/Windows NT (\d+)\.(\d+)/);
  if (windows)
    return os({
      platform: 'Windows',
      platformVersion: `${windows[1]}.${windows[2]}.0`,
      architecture: 'x86',
      bitness: /Win64|x64|WOW64/.test(userAgent) ? '64' : '32',
    });
  if (/\bCrOS\b/.test(userAgent))
    return os({
      platform: 'Chrome OS',
      architecture: 'x86',
      bitness: '64',
    });
  const mac = userAgent.match(/Mac OS X (\d+(?:[_.]\d+)*)/);
  if (mac || /\bMacintosh\b/.test(userAgent))
    return os({
      platform: 'macOS',
      platformVersion: (mac?.[1] ?? '').replaceAll('_', '.'),
    });
  if (/\bLinux\b|\bX11\b/.test(userAgent))
    return os({
      platform: 'Linux',
      architecture: /aarch64|arm/.test(userAgent) ? 'arm' : 'x86',
      bitness: /i[3-6]86/.test(userAgent) ? '32' : '64',
    });
  return null;
}

/**
 * The override that presents a host browser without headless markers. A custom user
 * agent naming another OS than the host keeps its own platform, mobile flag and OS-bound
 * client hints instead of the host's, so the string and the hints describe one machine.
 * One naming no known OS drops the hints, which would otherwise describe the host.
 * navigator.platform stays the host's: Chrome applies that override to pages but not to
 * workers, so changing it would make a worker disagree with its page.
 */
export function hostIdentity(probe: T.HostProbe): T.HostIdentity {
  const { userAgent, platform, metadata } = probe;
  const { brands, fullVersionList, ...rest } = metadata;
  const headed = (list: T.BrandVersion[] = []) =>
    list.map(({ brand, version }) => ({
      brand: brand.replace(/^HeadlessChrome$/, 'Google Chrome'),
      version,
    }));
  const named = userAgentOs(userAgent);
  const foreign = named !== null && named.platform !== metadata.platform;
  return {
    userAgent: userAgent.replace(/HeadlessChrome\//, 'Chrome/'),
    platform,
    userAgentMetadata: named
      ? {
          ...rest,
          ...(foreign && {
            platform: named.platform,
            platformVersion: named.platformVersion,
            architecture: named.architecture,
            bitness: named.bitness,
            model: named.model,
            mobile: named.mobile,
            wow64: false,
            formFactors: [named.mobile ? 'Mobile' : 'Desktop'],
          }),
          brands: headed(brands),
          fullVersionList: headed(fullVersionList),
        }
      : undefined,
  };
}

async function overrideUserAgent(
  cdp: T.StealthSession,
  identity: T.HostIdentity,
  sessionId?: string
) {
  await cdp.send(
    'Network.setUserAgentOverride',
    {
      userAgent: identity.userAgent,
      platform: identity.platform,
      userAgentMetadata: identity.userAgentMetadata,
    },
    sessionId
  );
}

function propagateToChildTargets(
  cdp: T.StealthSession,
  identity: T.HostIdentity
) {
  const failures: T.TargetFailure[] = [];
  const onAttached = async (event: T.AttachedToTarget) => {
    const { sessionId, targetInfo, waitingForDebugger } = event;
    try {
      if (!CHILD_TYPES.has(targetInfo?.type)) return;
      // Dedicated workers inherit their parent's override and have no Network domain.
      await overrideUserAgent(cdp, identity, sessionId).catch(error => {
        if (targetInfo.type !== 'worker') throw error;
      });
      if (targetInfo.type === 'iframe')
        await cdp.send('Target.setAutoAttach', STEALTH_AUTO_ATTACH, sessionId);
    } catch (error) {
      const message = errorMessage(error);
      failures.push({ targetId: targetInfo?.targetId, error: message });
      console.log(
        `[FINDING] STEALTH_CHILD_UNPATCHED type=${targetInfo?.type} target=${targetInfo?.targetId} ${message}`
      );
    } finally {
      if (waitingForDebugger)
        await cdp
          .send('Runtime.runIfWaitingForDebugger', {}, sessionId)
          .catch(() => {});
    }
  };
  cdp.on('Target.attachedToTarget', onAttached);
  return failures;
}

export async function applyStealthPatches(cdp: T.StealthSession) {
  if (cdp.stealthIdentity) return cdp.stealthIdentity;
  const identity = await readHostIdentity(cdp);
  await overrideUserAgent(cdp, identity);
  await cdp.send('Emulation.setFocusEmulationEnabled', { enabled: true });
  const { result } = await cdp.send('Runtime.evaluate', {
    expression:
      '({ width: outerWidth, height: outerHeight, screenWidth: screen.width, screenHeight: screen.height })',
    returnByValue: true,
  });
  const win = result?.value ?? {};
  if (win.screenWidth < win.width || win.screenHeight < win.height) {
    // A screen smaller than its window only happens headless. width/height 0 keep the
    // native viewport; only the reported screen changes.
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width: 0,
      height: 0,
      deviceScaleFactor: 0,
      mobile: false,
      screenWidth: Math.max(DESKTOP_SCREEN.width, win.width),
      screenHeight: Math.max(DESKTOP_SCREEN.height, win.height),
    });
  }
  cdp.stealthChildFailures = propagateToChildTargets(cdp, identity);
  await cdp.send('Target.setAutoAttach', STEALTH_AUTO_ATTACH);
  cdp.stealthAutoAttach = STEALTH_AUTO_ATTACH;
  cdp.stealthIdentity = identity;
  console.log('[INJECT] Stealth overrides applied');
  return identity;
}

const PAGE_PROBE = `(async () => {
  const nav = navigator;
  const webglRenderer = (() => {
    try {
      const gl = document.createElement('canvas').getContext('webgl');
      const ext = gl && gl.getExtension('WEBGL_debug_renderer_info');
      return ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : null;
    } catch { return null; }
  })();
  const worker = await new Promise(resolve => {
    try {
      const url = URL.createObjectURL(new Blob(
        ['postMessage({ userAgent: navigator.userAgent, platform: navigator.platform })'],
        { type: 'text/javascript' }
      ));
      const w = new Worker(url);
      const done = value => { w.terminate(); URL.revokeObjectURL(url); resolve(value); };
      w.onmessage = event => done(event.data);
      w.onerror = () => done(null);
      setTimeout(() => done(null), 2000);
    } catch { resolve(null); }
  });
  return {
    webdriver: nav.webdriver,
    userAgent: nav.userAgent,
    platform: nav.platform,
    uaDataPlatform: nav.userAgentData ? nav.userAgentData.platform : null,
    uaDataBrands: nav.userAgentData ? nav.userAgentData.brands.map(b => b.brand) : null,
    pluginCount: nav.plugins.length,
    languageCount: nav.languages.length,
    screen: [screen.width, screen.height],
    outer: [outerWidth, outerHeight],
    hasFocus: document.hasFocus(),
    chrome: typeof window.chrome,
    webglRenderer,
    // Opaque origins (about:blank, data:) deny permissions in every Chrome.
    notificationPermission: typeof Notification === 'undefined' || origin === 'null' ? null : Notification.permission,
    worker,
  };
})()`;

export async function verifyStealth(cdp: T.StealthSession) {
  const identity = cdp.stealthIdentity;
  // An isolated world keeps the probe invisible to page scripts and unaffected by them.
  const { frameTree } = await cdp.send('Page.getFrameTree');
  const { executionContextId } = await cdp.send('Page.createIsolatedWorld', {
    frameId: frameTree.frame.id,
    worldName: 'octocode-stealth-check',
  });
  const { result, exceptionDetails } = await cdp.send('Runtime.evaluate', {
    expression: PAGE_PROBE,
    contextId: executionContextId,
    awaitPromise: true,
    returnByValue: true,
  });
  if (exceptionDetails || !result?.value)
    throw new Error(
      `Stealth self-test could not read the page: ${exceptionDetails?.exception?.description ?? 'no result'}`
    );
  const v = result.value;
  const checks: [string, boolean, string][] = [
    [
      'navigator.webdriver',
      v.webdriver === false,
      `expected false, got ${v.webdriver}`,
    ],
    [
      'userAgent has no headless marker',
      !/Headless/i.test(v.userAgent),
      `got ${v.userAgent}`,
    ],
    [
      'userAgent matches override',
      !identity || v.userAgent === identity.userAgent,
      `expected ${identity?.userAgent}, got ${v.userAgent}`,
    ],
    [
      'navigator.platform matches host',
      !identity || v.platform === identity.platform,
      `expected ${identity?.platform}, got ${v.platform}`,
    ],
    [
      'userAgentData matches override (secure contexts)',
      v.uaDataPlatform === null ||
        ((!identity ||
          v.uaDataPlatform === (identity.userAgentMetadata?.platform ?? '')) &&
          // A user agent naming no known OS drops the hints, brands included.
          (v.uaDataBrands.length > 0 ||
            (identity !== undefined && !identity.userAgentMetadata)) &&
          !v.uaDataBrands.some((b: string) => /Headless/i.test(b))),
      `got platform=${v.uaDataPlatform} brands=${v.uaDataBrands}`,
    ],
    [
      'worker matches page',
      v.worker === null ||
        (v.worker.userAgent === v.userAgent &&
          v.worker.platform === v.platform),
      `worker ${JSON.stringify(v.worker)}`,
    ],
    [
      'screen covers window',
      v.screen[0] >= v.outer[0] && v.screen[1] >= v.outer[1],
      `screen ${v.screen.join('x')} < window ${v.outer.join('x')}`,
    ],
    [
      'document.hasFocus()',
      v.hasFocus === true,
      `expected true, got ${v.hasFocus}`,
    ],
    [
      'navigator.plugins',
      v.pluginCount > 0,
      `expected PDF plugins, got ${v.pluginCount}`,
    ],
    [
      'navigator.languages',
      v.languageCount > 0,
      `expected languages, got ${v.languageCount}`,
    ],
    ['window.chrome', v.chrome === 'object', `got ${v.chrome}`],
    [
      'Notification.permission',
      v.notificationPermission !== 'denied',
      `expected non-"denied", got ${v.notificationPermission}`,
    ],
  ];

  const failures: string[] = [];
  for (const [name, ok, msg] of checks)
    if (!ok) {
      console.log(`[FINDING] STEALTH_FAIL: ${name} - ${msg}`);
      failures.push(name);
    }
  const failed = failures.length,
    passed = checks.length - failed;
  // A software renderer is a real tell, but faking a GPU would contradict every other
  // WebGL value; report it without failing the run.
  if (/SwiftShader|llvmpipe/i.test(v.webglRenderer ?? ''))
    console.log(
      `[FINDING] STEALTH_WARN: software WebGL renderer is visible (${v.webglRenderer}); run headful or with a GPU when a site checks it`
    );
  console.log(
    `[INJECT] Stealth self-test: ${passed}/${checks.length} passed${failed > 0 ? ` - ${failed} FAILED` : ' - overrides verified'}`
  );
  return { passed, failed, total: checks.length, failures };
}
