// The one CDP safety policy: the table is the contract; everything else is free.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  CDP_POLICY_TABLE,
  classify,
  policyRefusal,
  scriptRefusal,
  SCRIPT_HARD_STOP_METHODS,
  parseWrappedCall,
  wrappedCallRefusal,
  twinEntries,
  URL_SCHEME_POLICY,
  urlScheme,
  urlSchemeRefusal,
  navigationParamRefusal,
} from '../../dist/engine/cdp-policy.mjs';

test('every table entry classifies as declared and has a reason', () => {
  assert.ok(CDP_POLICY_TABLE.length > 0);
  const seen = new Set();
  for (const entry of CDP_POLICY_TABLE) {
    assert.ok(!seen.has(entry.method), `duplicate ${entry.method}`);
    seen.add(entry.method);
    assert.match(entry.method, /^[A-Z][A-Za-z]*\.[A-Za-z]+$/);
    assert.equal(classify(entry.method), entry.policy);
    assert.equal(entry.policy, 'needsAck', 'there is no blocked tier');
    assert.ok(entry.reason.length > 0, `${entry.method} has no reason`);
  }
});

test('unlisted methods are free and never refused', () => {
  for (const method of ['DOM.querySelector', 'Runtime.enable', 'Nope.nope']) {
    assert.equal(classify(method), 'free');
    assert.equal(policyRefusal(method, false), undefined);
  }
});

test('every table method is refused without acknowledge and sent with it', () => {
  for (const { method, reason } of CDP_POLICY_TABLE) {
    assert.match(policyRefusal(method, false), /user's permission/);
    assert.match(policyRefusal(method, undefined), /acknowledge: true/);
    assert.ok(policyRefusal(method, false).includes(reason), method);
    assert.equal(policyRefusal(method, true), undefined);
  }
});

test('the former blocked methods need acknowledge and spell their consequence', () => {
  for (const method of [
    'Browser.close',
    'Browser.crash',
    'Browser.crashGpuProcess',
    'Page.crash',
    'Page.close',
    'Security.setIgnoreCertificateErrors',
    'Target.setRemoteLocations',
    'Target.exposeDevToolsProtocol',
  ]) {
    assert.equal(classify(method), 'needsAck', method);
    assert.doesNotMatch(policyRefusal(method, false), /is blocked/);
    assert.equal(policyRefusal(method, true), undefined, method);
  }
  assert.match(policyRefusal('Browser.close', false), /every tab/);
});

test('scripts keep the one hard stop: the former blocked set, never the wrapped call either', () => {
  assert.ok(SCRIPT_HARD_STOP_METHODS.includes('Target.sendMessageToTarget'));
  for (const method of SCRIPT_HARD_STOP_METHODS) {
    assert.match(scriptRefusal(method), /refused in scripts/, method);
    assert.match(scriptRefusal(method), /acknowledge: true/, method);
  }
  for (const method of [
    'Runtime.evaluate',
    'Network.getCookies',
    'DOM.getDocument',
  ])
    assert.equal(scriptRefusal(method), undefined, method);
  // The hard stop is exactly the methods whose consequence is the browser or the protocol itself.
  for (const method of SCRIPT_HARD_STOP_METHODS)
    assert.ok(
      classify(method) === 'needsAck' ||
        method === 'Target.sendMessageToTarget',
      method
    );
});

const wrap = (method, params = {}) => ({
  sessionId: 's',
  message: JSON.stringify({ id: 1, method, params }),
});

test('a wrapped call is judged as the direct call would be, malformed ones are refused', () => {
  assert.deepEqual(parseWrappedCall(wrap('DOM.getDocument', { depth: 1 })), {
    ok: true,
    method: 'DOM.getDocument',
    params: { depth: 1 },
  });
  for (const [params, reason] of [
    [undefined, /message must be a JSON string/],
    [{ message: 5 }, /message must be a JSON string/],
    [{ message: '{nope' }, /not valid JSON/],
    [{ message: '[1]' }, /not a JSON object/],
    [{ message: '{"id":1}' }, /no string "method"/],
  ]) {
    const refusal = wrappedCallRefusal(
      'Target.sendMessageToTarget',
      params,
      true
    );
    assert.match(refusal, reason);
    assert.match(refusal, /\[POLICY\]/);
  }
  assert.equal(wrappedCallRefusal('DOM.getDocument', {}, false), undefined);
  assert.equal(
    wrappedCallRefusal(
      'Target.sendMessageToTarget',
      wrap('DOM.getDocument'),
      false
    ),
    undefined
  );
  // Same verdicts as a direct call, with and without acknowledge, nested too.
  for (const { method } of CDP_POLICY_TABLE) {
    const direct = policyRefusal(method, false);
    assert.equal(
      wrappedCallRefusal('Target.sendMessageToTarget', wrap(method), false),
      direct
    );
    assert.equal(
      wrappedCallRefusal(
        'Target.sendMessageToTarget',
        wrap('Target.sendMessageToTarget', wrap(method)),
        false
      ),
      direct
    );
    assert.equal(
      wrappedCallRefusal('Target.sendMessageToTarget', wrap(method), true),
      undefined
    );
  }
  assert.ok(
    wrappedCallRefusal(
      'Target.sendMessageToTarget',
      wrap('Page.navigate', { url: 'file:///etc/hosts' }),
      false
    )
  );
});

test('the docs mention every policy entry', () => {
  const docs = readFileSync(
    join(import.meta.dirname, '../../docs/cdp-protocol.md'),
    'utf8'
  );
  for (const { method } of CDP_POLICY_TABLE)
    assert.ok(docs.includes(method), `${method} missing from docs`);
});

const schemaOf = commands => ({
  domains: Object.entries(commands).map(([domain, list]) => ({
    domain,
    commands: list,
  })),
});

test('a deprecated twin inherits the policy of the method it redirects to, through chains and by member name', () => {
  const schema = schemaOf({
    Old: [
      { name: 'setCookie', deprecated: true, redirect: 'Network' },
      {
        name: 'hop',
        deprecated: true,
        redirect: { domain: 'Mid', name: 'step' },
      },
      { name: 'setDownloadBehavior', deprecated: true },
      { name: 'unrelated', deprecated: true, redirect: 'Emulation' },
    ],
    Mid: [{ name: 'step', deprecated: true, redirect: 'Browser' }],
    Network: [{ name: 'setCookie' }],
    Browser: [{ name: 'step' }, { name: 'close' }],
    Emulation: [{ name: 'unrelated' }],
  });
  const twins = twinEntries(schema);
  assert.equal(classify('Old.setCookie', twins), 'needsAck');
  assert.match(
    policyRefusal('Old.setCookie', false, twins),
    /twin of Network\.setCookie/
  );
  assert.equal(policyRefusal('Old.setCookie', true, twins), undefined);
  assert.equal(classify('Old.setDownloadBehavior', twins), 'needsAck');
  assert.equal(classify('Old.unrelated', twins), 'free');
  assert.equal(classify('Old.setCookie'), 'free', 'twins need the schema');
  const chained = twinEntries(
    schemaOf({
      Old: [{ name: 'close', deprecated: true, redirect: 'Browser' }],
      Browser: [{ name: 'close' }],
    })
  );
  assert.match(policyRefusal('Old.close', false, chained), /user's permission/);
  // A twin never replaces what the table already lists.
  assert.equal(twinEntries(schema).has('Page.setGeolocationOverride'), false);
});

test('the static table already covers the known deprecated twins offline', () => {
  for (const method of [
    'Page.setGeolocationOverride',
    'Page.deleteCookie',
    'Page.setDownloadBehavior',
    'Page.addScriptToEvaluateOnLoad',
  ])
    assert.notEqual(classify(method), 'free', method);
});

test('every refused URL scheme is refused, allowed ones pass, and the docs list each scheme', () => {
  for (const { scheme, reason } of URL_SCHEME_POLICY) {
    const url = `${scheme}:x`;
    assert.equal(urlScheme(url), scheme);
    assert.ok(reason.length > 0);
    const noConsent = urlSchemeRefusal(url, 'goto', null);
    assert.match(noConsent, new RegExp(`${scheme}: URLs are refused`));
    assert.match(noConsent, /Page\.navigate/);
    assert.match(urlSchemeRefusal(url, 'cdp', false), /acknowledge: true/);
    assert.equal(urlSchemeRefusal(url, 'cdp', true), undefined);
    assert.match(
      navigationParamRefusal('Page.navigate', { url }, undefined),
      /Page\.navigate: /
    );
    assert.equal(
      navigationParamRefusal('Page.navigate', { url }, true),
      undefined
    );
  }
  for (const url of [
    'https://example.com',
    'http://127.0.0.1:1/file:///',
    'data:text/html,<p>x</p>',
    'about:blank',
    'about:blank#a',
    '/relative/file',
  ])
    assert.equal(urlSchemeRefusal(url, 'goto', null), undefined, url);
  // Chrome ignores leading control characters, tabs, newlines and letter case in a scheme.
  for (const url of [
    '  FILE:///etc/hosts',
    '\u0001file:///x',
    'fi\tle:///x',
    'JavaScript:1',
  ])
    assert.ok(urlSchemeRefusal(url, 'goto', null), JSON.stringify(url));
  assert.equal(
    navigationParamRefusal('DOM.querySelector', { url: 'file:///x' }, false),
    undefined
  );
  assert.ok(
    navigationParamRefusal('Target.createTarget', { url: 'file:///x' }, false)
  );
  for (const doc of ['cdp-protocol.md', 'tabs-and-policy.md']) {
    const text = readFileSync(
      join(import.meta.dirname, '../../docs', doc),
      'utf8'
    );
    for (const { scheme } of URL_SCHEME_POLICY)
      assert.ok(text.includes(`${scheme}:`), `${scheme}: missing from ${doc}`);
  }
});

test('the docs table lists each entry with its reason (a deprecated twin row names its base)', () => {
  const docs = readFileSync(
    join(import.meta.dirname, '../../docs/cdp-protocol.md'),
    'utf8'
  );
  for (const { method, reason } of CDP_POLICY_TABLE)
    if (!/deprecated twin of/.test(reason))
      assert.ok(
        docs.includes(`| \`${method}\` | ${reason} |`),
        `${method} row missing or stale in docs/cdp-protocol.md`
      );
});

test('schema-derived additions need permission, by domain; device, media, CPU and touch emulation stay free', () => {
  const gated = {
    Storage: [
      'getTrustTokens',
      'getPrivateVerificationTokens',
      'overrideQuotaForOrigin',
      'deleteStorageBucket',
      'clearTrustTokens',
      'clearDataForStorageKey',
      'clearDataForOrigin',
    ],
    IndexedDB: [
      'requestData',
      'getMetadata',
      'clearObjectStore',
      'deleteDatabase',
      'deleteObjectStoreEntries',
    ],
    CacheStorage: [
      'requestCachedResponse',
      'requestEntries',
      'deleteCache',
      'deleteEntry',
    ],
    DOMStorage: [
      'getDOMStorageItems',
      'setDOMStorageItem',
      'removeDOMStorageItem',
      'clear',
    ],
    Input: [
      'dispatchKeyEvent',
      'dispatchMouseEvent',
      'dispatchTouchEvent',
      'dispatchDragEvent',
      'insertText',
      'imeSetComposition',
      'synthesizePinchGesture',
      'synthesizeScrollGesture',
      'synthesizeTapGesture',
      'setIgnoreInputEvents',
      'emulateTouchFromMouseEvent',
    ],
    Autofill: ['trigger', 'setAddresses'],
    Extensions: [
      'loadUnpacked',
      'uninstall',
      'clearStorageItems',
      'setStorageItems',
    ],
    PWA: ['install', 'launch', 'uninstall'],
    FedCm: ['selectAccount', 'dismissDialog', 'clickDialogButton', 'openUrl'],
    DigitalCredentials: ['setVirtualWalletBehavior'],
    WebAuthn: [
      'addCredential',
      'getCredentials',
      'removeCredential',
      'clearCredentials',
      'addVirtualAuthenticator',
    ],
    Fetch: [
      'fulfillRequest',
      'failRequest',
      'continueRequest',
      'continueWithAuth',
      'getResponseBody',
    ],
    Network: [
      'getResponseBody',
      'loadNetworkResource',
      'setCacheDisabled',
      'emulateNetworkConditions',
      'overrideNetworkState',
      'replayXHR',
      'setBlockedURLs',
    ],
    Debugger: [
      'setScriptSource',
      'setVariableValue',
      'evaluateOnCallFrame',
      'setReturnValue',
    ],
    Page: [
      'setInterceptFileChooserDialog',
      'setWebLifecycleState',
      'navigateToHistoryEntry',
      'generateTestReport',
      'setDocumentContent',
      'setBypassCSP',
    ],
    Emulation: [
      'setUserAgentOverride',
      'setScriptExecutionDisabled',
      'setTimezoneOverride',
      'setSensorOverrideReadings',
    ],
    Browser: [
      'setWindowBounds',
      'setContentsSize',
      'addPrivacySandboxEnrollmentOverride',
      'getBrowserCommandLine',
      'executeBrowserCommand',
    ],
    Target: ['setAutoAttach', 'disposeBrowserContext'],
    Tethering: ['bind', 'unbind'],
    FileSystem: ['getDirectory'],
  };
  for (const [domain, names] of Object.entries(gated))
    for (const name of names)
      assert.equal(
        classify(`${domain}.${name}`),
        'needsAck',
        `${domain}.${name}`
      );
  // Virtual hardware: every command of both domains, enable and disable included.
  const table = new Set(CDP_POLICY_TABLE.map(entry => entry.method));
  for (const method of [
    'BluetoothEmulation.enable',
    'BluetoothEmulation.simulateAdvertisement',
    'SmartCardEmulation.enable',
    'SmartCardEmulation.reportError',
  ])
    assert.ok(table.has(method), method);
  for (const method of [
    'Emulation.setDeviceMetricsOverride',
    'Emulation.clearDeviceMetricsOverride',
    'Emulation.setEmulatedMedia',
    'Emulation.setCPUThrottlingRate',
    'Emulation.setTouchEmulationEnabled',
    'Emulation.setLocaleOverride',
    'Overlay.highlightNode',
    'Overlay.enable',
    'Browser.getHistograms',
    'Browser.getVersion',
  ])
    assert.equal(classify(method), 'free', method);
});

test('enable and disable never make a deprecated domain a twin of another domain', () => {
  const schema = schemaOf({
    BluetoothEmulation: [{ name: 'enable' }, { name: 'disable' }],
    Old: [
      { name: 'enable', deprecated: true },
      { name: 'disable', deprecated: true },
      { name: 'setCookie', deprecated: true },
    ],
  });
  assert.equal(classify('BluetoothEmulation.enable'), 'needsAck');
  const twins = twinEntries(schema);
  assert.equal(classify('Old.enable', twins), 'free');
  assert.equal(classify('Old.disable', twins), 'free');
  assert.equal(
    classify('Old.setCookie', twins),
    'needsAck',
    'other names still match'
  );
});
