// The one CDP safety policy: every raw method an agent sends is classified here and nowhere else.
// Unlisted methods are free; the installed Chrome's schema (cdp-schema.mts) decides whether they exist.

export type CdpPolicy = 'free' | 'needsAck';
export type PolicyEntry = {
  method: string;
  policy: 'needsAck';
  /** One line shown to the agent in the refusal and in the docs; spells the consequence. */
  reason: string;
};

const needsAck = (method: string, reason: string): PolicyEntry => ({
  method,
  policy: 'needsAck',
  reason,
});

export const CDP_POLICY_TABLE: readonly PolicyEntry[] = [
  needsAck(
    'Browser.close',
    'closes the whole browser and every tab in it (prefer cleanup for a browser this package launched)'
  ),
  needsAck(
    'Browser.crash',
    'deliberately crashes the browser process and every tab'
  ),
  needsAck(
    'Browser.crashGpuProcess',
    'deliberately crashes the GPU process, which can blank every tab'
  ),
  needsAck(
    'Page.crash',
    'deliberately crashes the page process and loses its state (use goto or reload to reset a page)'
  ),
  needsAck(
    'Page.close',
    'closes the page without releasing engine state (prefer --close-tab, or cleanup for a browser this package launched)'
  ),
  needsAck(
    'Security.setIgnoreCertificateErrors',
    'turns off certificate validation for every page (prefer fixing the certificate)'
  ),
  needsAck(
    'Target.setRemoteLocations',
    'exposes the browser to remote debugging locations (prefer loopback only)'
  ),
  needsAck(
    'Target.exposeDevToolsProtocol',
    'hands the DevTools protocol to page scripts'
  ),
  needsAck(
    'Browser.grantPermissions',
    'grants site permissions (camera, location, clipboard...)'
  ),
  needsAck(
    'Runtime.evaluate',
    'runs arbitrary JavaScript in the page; for reading state use the evaluate plan op'
  ),
  needsAck(
    'Runtime.callFunctionOn',
    'runs arbitrary JavaScript in the page; for reading state use the evaluate plan op'
  ),
  needsAck(
    'Runtime.compileScript',
    'compiles arbitrary JavaScript in the page; for reading state use the evaluate plan op'
  ),
  needsAck(
    'Runtime.runScript',
    'runs arbitrary JavaScript in the page; for reading state use the evaluate plan op'
  ),
  needsAck('Browser.setPermission', 'changes a site permission'),
  needsAck('Browser.resetPermissions', 'resets every site permission'),
  needsAck('Browser.setDownloadBehavior', 'lets pages write files to disk'),
  needsAck('Network.setCookie', 'writes a cookie into the profile'),
  needsAck('Network.setCookies', 'writes cookies into the profile'),
  needsAck('Network.deleteCookies', 'deletes cookies from the profile'),
  needsAck('Network.clearBrowserCookies', 'deletes every cookie'),
  needsAck(
    'Network.getAllCookies',
    'reads every cookie value, secrets included'
  ),
  needsAck('Network.getCookies', 'reads cookie values, secrets included'),
  needsAck('Network.setExtraHTTPHeaders', 'adds headers to every request'),
  needsAck(
    'Network.setRequestInterception',
    'intercepts and can alter requests'
  ),
  needsAck('Storage.getCookies', 'reads cookie values, secrets included'),
  needsAck('Storage.setCookies', 'writes cookies into the profile'),
  needsAck('Storage.clearCookies', 'deletes cookies from the profile'),
  needsAck('Storage.clearDataForOrigin', 'erases stored site data'),
  needsAck('DOMStorage.setDOMStorageItem', 'writes site storage'),
  needsAck('IndexedDB.clearObjectStore', 'erases stored site data'),
  needsAck('IndexedDB.deleteDatabase', 'erases a site database'),
  needsAck('CacheStorage.deleteCache', 'erases a site cache'),
  needsAck('Page.setBypassCSP', 'disables the page content security policy'),
  needsAck(
    'Page.addScriptToEvaluateOnNewDocument',
    'runs a script in every later document of the page'
  ),
  needsAck(
    'Emulation.setGeolocationOverride',
    'changes the location sites see'
  ),
  // Deprecated twins stay classified offline; twinEntries() derives the same from a live schema.
  needsAck(
    'Page.setGeolocationOverride',
    'changes the location sites see (deprecated twin of Emulation.setGeolocationOverride)'
  ),
  needsAck(
    'Page.deleteCookie',
    'deletes a cookie from the profile (deprecated twin of Network.deleteCookies)'
  ),
  needsAck(
    'Page.setDownloadBehavior',
    'lets pages write files to disk (deprecated twin of Browser.setDownloadBehavior)'
  ),
  needsAck(
    'Page.addScriptToEvaluateOnLoad',
    'runs a script in every later document of the page (deprecated twin of Page.addScriptToEvaluateOnNewDocument)'
  ),
  needsAck(
    'Security.setOverrideCertificateErrors',
    'changes how certificate errors are handled for every page'
  ),
  needsAck(
    'Security.handleCertificateError',
    'continues past a certificate error'
  ),
  needsAck('WebAuthn.addVirtualAuthenticator', 'adds a fake authenticator'),
  needsAck('WebAuthn.addCredential', 'stores a credential'),
  needsAck('WebAuthn.getCredentials', 'reads stored credentials'),
  needsAck('Fetch.fulfillRequest', 'answers a request with a made-up reply'),
  needsAck('Fetch.continueRequest', 'modifies traffic in flight'),
  needsAck('Fetch.failRequest', 'drops traffic in flight'),
  // Schema-derived additions: writes, secret reads, virtual hardware and raw input. Overlay and device metrics, media, CPU and touch stay free.
  needsAck(
    'Storage.getTrustTokens',
    'reads Private State Token issuers and counts the browser holds'
  ),
  needsAck(
    'Storage.getPrivateVerificationTokens',
    'reads the private verification tokens the browser holds'
  ),
  needsAck(
    'Storage.overrideQuotaForOrigin',
    'changes the storage quota a site sees'
  ),
  needsAck(
    'Storage.deleteStorageBucket',
    'erases a storage bucket (known to crash some Chrome builds)'
  ),
  needsAck('Storage.clearTrustTokens', 'erases Private State Tokens'),
  needsAck('Storage.clearDataForStorageKey', 'erases stored site data'),
  needsAck(
    'IndexedDB.requestData',
    'reads stored site records, secrets included'
  ),
  needsAck('IndexedDB.getMetadata', 'reads site database metadata'),
  needsAck(
    'IndexedDB.deleteObjectStoreEntries',
    'erases records of a site database'
  ),
  needsAck(
    'CacheStorage.requestCachedResponse',
    'reads a cached response body'
  ),
  needsAck('CacheStorage.requestEntries', 'lists the entries of a site cache'),
  needsAck('CacheStorage.deleteEntry', 'erases an entry of a site cache'),
  needsAck(
    'DOMStorage.getDOMStorageItems',
    'reads site storage values, secrets included'
  ),
  needsAck('DOMStorage.removeDOMStorageItem', 'erases a site storage item'),
  needsAck('DOMStorage.clear', 'erases a site storage'),
  needsAck(
    'Input.dispatchKeyEvent',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.dispatchMouseEvent',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.dispatchTouchEvent',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.dispatchDragEvent',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.insertText',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.imeSetComposition',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.synthesizePinchGesture',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.synthesizeScrollGesture',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.synthesizeTapGesture',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.setIgnoreInputEvents',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck(
    'Input.emulateTouchFromMouseEvent',
    'sends raw input to the page without the act operation actionability checks (prefer act)'
  ),
  needsAck('Autofill.trigger', 'fills a form with stored autofill data'),
  needsAck(
    'Autofill.setAddresses',
    'replaces the autofill addresses the browser holds'
  ),
  needsAck('Extensions.loadUnpacked', 'installs an extension from disk'),
  needsAck('Extensions.uninstall', 'removes an extension'),
  needsAck('Extensions.clearStorageItems', 'changes extension storage'),
  needsAck('Extensions.setStorageItems', 'changes extension storage'),
  needsAck('PWA.install', 'installs a web app on the system'),
  needsAck('PWA.launch', 'launches an installed web app'),
  needsAck('PWA.uninstall', 'uninstalls a web app'),
  needsAck(
    'FedCm.selectAccount',
    'signs in through the federated login dialog'
  ),
  needsAck('FedCm.dismissDialog', 'dismisses the federated login dialog'),
  needsAck('FedCm.clickDialogButton', 'clicks a federated login dialog button'),
  needsAck('FedCm.openUrl', 'opens a link from the federated login dialog'),
  needsAck(
    'DigitalCredentials.setVirtualWalletBehavior',
    'installs a virtual digital wallet that answers credential requests'
  ),
  needsAck('WebAuthn.removeCredential', 'removes stored credentials'),
  needsAck('WebAuthn.clearCredentials', 'removes stored credentials'),
  needsAck(
    'BluetoothEmulation.enable',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.setSimulatedCentralState',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.disable',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.simulatePreconnectedPeripheral',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.simulateAdvertisement',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.simulateGATTOperationResponse',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.simulateCharacteristicOperationResponse',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.simulateDescriptorOperationResponse',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.addService',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.removeService',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.addCharacteristic',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.removeCharacteristic',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.addDescriptor',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.removeDescriptor',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'BluetoothEmulation.simulateGATTDisconnection',
    'drives virtual Bluetooth hardware the page can see'
  ),
  needsAck(
    'SmartCardEmulation.enable',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.disable',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportEstablishContextResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportReleaseContextResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportListReadersResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportGetStatusChangeResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportBeginTransactionResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportPlainResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportConnectResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportDataResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportStatusResult',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'SmartCardEmulation.reportError',
    'drives a virtual smart card reader the page can see'
  ),
  needsAck(
    'Fetch.continueWithAuth',
    'answers an authentication challenge in flight'
  ),
  needsAck('Fetch.getResponseBody', 'reads a paused response body'),
  needsAck(
    'Network.getResponseBody',
    'reads a response body, secrets included'
  ),
  needsAck(
    'Network.loadNetworkResource',
    'fetches a URL through the browser with its credentials'
  ),
  needsAck('Network.setCacheDisabled', 'changes caching for every request'),
  needsAck(
    'Network.emulateNetworkConditions',
    'changes the network conditions of every request'
  ),
  needsAck(
    'Network.overrideNetworkState',
    'changes the network conditions of every request'
  ),
  needsAck('Network.replayXHR', 're-sends a request, side effects included'),
  needsAck('Network.setBlockedURLs', 'blocks matching requests for every page'),
  needsAck('Debugger.setScriptSource', 'rewrites running script source'),
  needsAck('Debugger.setVariableValue', 'changes a variable of a paused frame'),
  needsAck(
    'Debugger.evaluateOnCallFrame',
    'runs arbitrary JavaScript in a paused frame'
  ),
  needsAck(
    'Debugger.setReturnValue',
    'changes the value a paused function returns'
  ),
  needsAck(
    'Page.setInterceptFileChooserDialog',
    'takes over the page file chooser'
  ),
  needsAck('Page.setWebLifecycleState', 'freezes or discards the page'),
  needsAck('Page.navigateToHistoryEntry', 'moves the page through its history'),
  needsAck(
    'Page.generateTestReport',
    'sends a report to the page reporting endpoint'
  ),
  needsAck('Page.setDocumentContent', 'replaces the page document'),
  needsAck(
    'Emulation.setUserAgentOverride',
    'changes the user agent every site sees'
  ),
  needsAck(
    'Emulation.setScriptExecutionDisabled',
    'stops the page from running scripts'
  ),
  needsAck('Emulation.setTimezoneOverride', 'changes the time zone sites see'),
  needsAck(
    'Emulation.setSensorOverrideReadings',
    'changes sensor readings sites see'
  ),
  needsAck('Browser.setWindowBounds', 'moves or resizes the browser window'),
  needsAck('Browser.setContentsSize', 'resizes the page contents'),
  needsAck(
    'Browser.addPrivacySandboxEnrollmentOverride',
    'changes Privacy Sandbox enrollment'
  ),
  needsAck(
    'Browser.getBrowserCommandLine',
    'reads the browser launch switches'
  ),
  needsAck('Browser.executeBrowserCommand', 'runs a browser command'),
  needsAck(
    'Target.setAutoAttach',
    'attaches to every new target (set it per target, not browser-wide)'
  ),
  needsAck(
    'Target.disposeBrowserContext',
    'destroys a browser context and its tabs'
  ),
  needsAck(
    'Tethering.bind',
    'opens or closes a port on the host that reaches the browser'
  ),
  needsAck(
    'Tethering.unbind',
    'opens or closes a port on the host that reaches the browser'
  ),
  needsAck(
    'FileSystem.getDirectory',
    'reads a directory handle of the file system'
  ),
];

/**
 * Methods whose effect lives on the connection that sent them: a later call (its own connection) does
 * not see them. A run that ends on one of these warns; persist with `open` emulation instead.
 */
export const CONNECTION_OVERRIDE_METHODS: readonly string[] = [
  'Emulation.setFocusEmulationEnabled',
  'Emulation.setAutoDarkModeOverride',
  'Emulation.setCPUThrottlingRate',
  'Emulation.setDefaultBackgroundColorOverride',
  'Emulation.setSafeAreaInsetsOverride',
  'Emulation.setVirtualKeyboardGeometryOverride',
  'Emulation.setDeviceMetricsOverride',
  'Emulation.setDevicePostureOverride',
  'Emulation.setDisplayFeaturesOverride',
  'Emulation.setScrollbarsHidden',
  'Emulation.setDocumentCookieDisabled',
  'Emulation.setEmitTouchEventsForMouse',
  'Emulation.setEmulatedMedia',
  'Emulation.setEmulatedVisionDeficiency',
  'Emulation.setEmulatedOSTextScale',
  'Emulation.setGeolocationOverride',
  'Emulation.setSensorOverrideEnabled',
  'Emulation.setSensorOverrideReadings',
  'Emulation.setPressureSourceOverrideEnabled',
  'Emulation.setPressureStateOverride',
  'Emulation.setIdleOverride',
  'Emulation.setNavigatorOverrides',
  'Emulation.setPageScaleFactor',
  'Emulation.setScriptExecutionDisabled',
  'Emulation.setTouchEmulationEnabled',
  'Emulation.setVirtualTimePolicy',
  'Emulation.setLocaleOverride',
  'Emulation.setTimezoneOverride',
  'Emulation.setVisibleSize',
  'Emulation.setDisabledImageTypes',
  'Emulation.setDataSaverOverride',
  'Emulation.setHardwareConcurrencyOverride',
  'Emulation.setCPUPerformanceOverride',
  'Emulation.setUserAgentOverride',
  'Emulation.setAutomationOverride',
  'Emulation.setSmallViewportHeightDifferenceOverride',
  'Emulation.setPrimaryScreen',
  'Network.setExtraHTTPHeaders',
  'Network.setCacheDisabled',
  'Network.setUserAgentOverride',
  'Network.setBlockedURLs',
  'Network.emulateNetworkConditions',
  'Page.addScriptToEvaluateOnNewDocument',
  'Page.setLifecycleEventsEnabled',
  'Page.setBypassCSP',
  'Fetch.enable',
  'Browser.grantPermissions',
  'Browser.setPermission',
];

/** The reply warning for a run whose last step is a connection-bound override; undefined otherwise. */
export const connectionOverrideNote = (method: string | undefined) =>
  method && CONNECTION_OVERRIDE_METHODS.includes(method)
    ? `[NOTE] ${method} applies only to this connection; keep the dependent steps in the same run, or persist with open emulation`
    : undefined;

const byMethod = new Map(CDP_POLICY_TABLE.map(entry => [entry.method, entry]));

/** The deprecated-twin entries a live schema adds to the table, keyed by method. */
export type TwinEntries = ReadonlyMap<string, PolicyEntry>;
type SchemaCommand = { name: string; deprecated?: boolean; redirect?: unknown };
/** The part of the live protocol schema the twin rule reads. */
export type TwinSchema = {
  domains: { domain: string; commands?: SchemaCommand[] }[];
};

/** The `Domain.name` a deprecated command redirects to: a domain string (same name) or {domain, name}. */
function redirectTarget(domain: string, command: SchemaCommand) {
  const { redirect } = command;
  if (typeof redirect === 'string') return `${redirect}.${command.name}`;
  if (typeof redirect === 'object' && redirect !== null) {
    const { domain: to, name } = redirect as Record<string, unknown>;
    if (typeof to === 'string')
      return `${to}.${typeof name === 'string' ? name : command.name}`;
  }
  return undefined;
}

/** Member names every domain has; a same-name match on them would make any deprecated domain inherit a gate. */
const LIFECYCLE = new Set(['enable', 'disable']);

const twinCache = new WeakMap<object, TwinEntries>();

/**
 * Every deprecated command that inherits the policy of the method it was replaced by: its `redirect`
 * (followed through chains), else a command of the same member name in another domain. A twin
 * never weakens or replaces an entry the table already lists.
 */
export function twinEntries(schema: TwinSchema): TwinEntries {
  const cached = twinCache.get(schema);
  if (cached) return cached;
  const commands = new Map<string, SchemaCommand>();
  for (const domain of schema.domains)
    for (const command of domain.commands ?? [])
      commands.set(`${domain.domain}.${command.name}`, command);
  const found = new Map<string, PolicyEntry>();
  for (const [method, command] of commands) {
    if (command.deprecated !== true || byMethod.has(method)) continue;
    const domain = method.slice(0, method.indexOf('.'));
    // enable and disable exist in every domain: they never identify a method across domains.
    const sameName = LIFECYCLE.has(command.name)
      ? undefined
      : [...byMethod.keys()].find(
          key => key.slice(key.indexOf('.') + 1) === command.name
        );
    let target = redirectTarget(domain, command) ?? sameName;
    for (let hop = 0; target && hop < 5; hop++) {
      const base = byMethod.get(target);
      if (base) {
        found.set(method, {
          ...base,
          method,
          reason: `${base.reason} (deprecated twin of ${target})`,
        });
        break;
      }
      const next = commands.get(target);
      target = next && redirectTarget(target.split('.')[0], next);
    }
  }
  twinCache.set(schema, found);
  return found;
}

export const policyEntry = (
  method: string,
  twins?: TwinEntries
): PolicyEntry | undefined => byMethod.get(method) ?? twins?.get(method);

/** The policy of one method; anything not in the table (or a derived twin) is free. */
export const classify = (method: string, twins?: TwinEntries): CdpPolicy =>
  policyEntry(method, twins)?.policy ?? 'free';

/** The refusal for a method that needs the user's permission and has none; undefined otherwise. */
export function policyRefusal(
  method: string,
  acknowledge: boolean | undefined,
  twins?: TwinEntries
): string | undefined {
  const entry = policyEntry(method, twins);
  if (entry?.policy !== 'needsAck' || acknowledge === true) return undefined;
  return `[POLICY] ${method} ${entry.reason} and needs the user's permission: ask them, then retry with acknowledge: true`;
}

/**
 * Methods a sandboxed runner script (`cdp.send`) may never send: a script cannot carry an
 * acknowledgement, so the methods that need one for their consequence stay refused there. This is the
 * one place a hard stop remains; scripts are raw by design. A direct `cdp` call may send them with
 * acknowledge: true.
 */
const SCRIPT_HARD_STOP: ReadonlyMap<string, string> = new Map([
  ['Browser.close', 'Use the cleanup command for a browser the engine opened.'],
  ['Browser.crash', 'Use the cleanup command for a browser the engine opened.'],
  [
    'Browser.crashGpuProcess',
    'Use the cleanup command for a browser the engine opened.',
  ],
  ['Page.crash', 'Use goto or reload to reset a page.'],
  [
    'Page.close',
    'Use --close-tab, or the cleanup command for a browser the engine opened.',
  ],
  [
    'Security.setIgnoreCertificateErrors',
    'Fix the certificate, or open the browser with the documented launch option.',
  ],
  ['Target.setRemoteLocations', 'Connect to the loopback port only.'],
  ['Target.exposeDevToolsProtocol', 'Send the method from the engine instead.'],
  [
    'Target.sendMessageToTarget',
    'It wraps a raw method; a script cannot acknowledge the wrapped call. Send the method itself.',
  ],
]);

/** The refusal of a script's `cdp.send`; undefined when the method may be sent from a script. */
export function scriptRefusal(method: string): string | undefined {
  const alternative = SCRIPT_HARD_STOP.get(method);
  return alternative === undefined
    ? undefined
    : `[POLICY] ${method} is refused in scripts (a script cannot ask the user): use a cdp call with acknowledge: true after the user agrees. ${alternative}`;
}

/** The methods `scriptRefusal` stops (docs and tests). */
export const SCRIPT_HARD_STOP_METHODS: readonly string[] = [
  ...SCRIPT_HARD_STOP.keys(),
];

// ---------- wrapped calls ----------

export type WrappedCall =
  { ok: true; method: string; params: unknown } | { ok: false; reason: string };

/** The raw call inside a `Target.sendMessageToTarget` `message`; the policy judges it as a direct call. */
export function parseWrappedCall(params: unknown): WrappedCall {
  const message =
    typeof params === 'object' && params !== null
      ? (params as Record<string, unknown>).message
      : undefined;
  if (typeof message !== 'string')
    return { ok: false, reason: 'params.message must be a JSON string' };
  let parsed: unknown;
  try {
    parsed = JSON.parse(message);
  } catch {
    return { ok: false, reason: 'params.message is not valid JSON' };
  }
  const inner = parsed as Record<string, unknown> | null;
  if (typeof inner !== 'object' || inner === null || Array.isArray(inner))
    return { ok: false, reason: 'params.message is not a JSON object' };
  if (typeof inner.method !== 'string' || !inner.method)
    return {
      ok: false,
      reason: 'params.message has no string "method" to classify',
    };
  return { ok: true, method: inner.method, params: inner.params ?? {} };
}

/** Refusal for a malformed wrapped call, else the inner method's policy; undefined for any other method. */
export function wrappedCallRefusal(
  method: string,
  params: unknown,
  acknowledge: boolean | undefined,
  twins?: TwinEntries
): string | undefined {
  if (method !== 'Target.sendMessageToTarget') return undefined;
  const inner = parseWrappedCall(params);
  if (!inner.ok)
    return `[POLICY] Target.sendMessageToTarget wraps a raw call and the wrapped call must be inspectable: ${inner.reason}`;
  return (
    wrappedCallRefusal(inner.method, inner.params, acknowledge, twins) ??
    policyRefusal(inner.method, acknowledge, twins) ??
    navigationParamRefusal(inner.method, inner.params, acknowledge)
  );
}

// ---------- URL schemes ----------

export type SchemePolicy = { scheme: string; reason: string };
/**
 * Schemes a navigation may not use without the user's consent. data: and about:blank stay allowed:
 * a data: page is the same as inline HTML the agent could already write, and about:blank is empty.
 */
export const URL_SCHEME_POLICY: readonly SchemePolicy[] = [
  { scheme: 'file', reason: 'read files from the local disk' },
  { scheme: 'chrome', reason: 'open browser internals' },
  { scheme: 'chrome-untrusted', reason: 'open browser internals' },
  { scheme: 'chrome-extension', reason: 'open extension pages and files' },
  { scheme: 'devtools', reason: 'open browser internals' },
  { scheme: 'view-source', reason: 'read a page source through the browser' },
  { scheme: 'javascript', reason: 'run a script in the current page' },
  {
    scheme: 'about',
    reason: 'open browser internals (only about:blank is allowed)',
  },
];

/** The lowercase scheme Chrome would parse from a url, or undefined; leading control/space and tabs/newlines are ignored as Chrome ignores them. */
export function urlScheme(url: string): string | undefined {
  let start = 0;
  while (start < url.length && url.charCodeAt(start) <= 0x20) start++;
  const text = url.slice(start).replace(/[\t\n\r]/g, '');
  return /^([a-z][a-z0-9+.-]*):/i.exec(text)?.[1].toLowerCase();
}

/** Navigation methods whose `url` param loads a page. */
const NAVIGATION_METHODS: readonly string[] = [
  'Page.navigate',
  'Target.createTarget',
  'Network.loadNetworkResource',
];

/** The refusal for a url that uses a scheme of the table; undefined when its scheme is allowed. */
export function urlSchemeRefusal(
  url: string,
  where: string,
  /** The call's acknowledge; null when this kind of call cannot carry one (goto, new tab, open url). */
  acknowledge: boolean | null | undefined
): string | undefined {
  const scheme = urlScheme(url);
  const entry = URL_SCHEME_POLICY.find(row => row.scheme === scheme);
  if (!entry) return undefined;
  if (scheme === 'about' && /^about:blank([?#].*)?$/i.test(url.trim()))
    return undefined;
  if (acknowledge === true) return undefined;
  const way =
    acknowledge === null
      ? 'ask the user, then navigate with a cdp call to Page.navigate and acknowledge: true'
      : 'ask the user, then retry with acknowledge: true';
  return `[POLICY] ${where}: ${scheme}: URLs are refused because they ${entry.reason}; ${way}`;
}

/** The refusal for a navigation method whose `url` param uses a refused scheme; undefined otherwise. */
export function navigationParamRefusal(
  method: string,
  params: unknown,
  acknowledge: boolean | undefined
): string | undefined {
  if (!NAVIGATION_METHODS.includes(method)) return undefined;
  const url =
    typeof params === 'object' && params !== null
      ? (params as Record<string, unknown>).url
      : undefined;
  return typeof url === 'string'
    ? urlSchemeRefusal(url, method, acknowledge === true)
    : undefined;
}
