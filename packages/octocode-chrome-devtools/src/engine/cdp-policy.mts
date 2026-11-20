// The one CDP safety policy: every raw method an agent sends is classified here and nowhere else.
// Unlisted methods are free; the installed Chrome's schema (cdp-schema.mts) decides whether they exist.

export type CdpPolicy = 'free' | 'needsAck' | 'blocked';
export type PolicyEntry = {
  method: string;
  policy: Exclude<CdpPolicy, 'free'>;
  /** One line shown to the agent in the refusal and in the docs. */
  reason: string;
  /** What to do instead, for a blocked method. */
  alternative?: string;
};

const blocked = (
  method: string,
  reason: string,
  alternative = 'Use the cleanup command to close a browser the engine opened.'
): PolicyEntry => ({ method, policy: 'blocked', reason, alternative });
const needsAck = (method: string, reason: string): PolicyEntry => ({
  method,
  policy: 'needsAck',
  reason,
});

export const CDP_POLICY_TABLE: readonly PolicyEntry[] = [
  blocked('Browser.close', 'closes the whole browser and every tab in it'),
  blocked('Browser.crash', 'deliberately crashes the browser process'),
  blocked('Browser.crashGpuProcess', 'deliberately crashes the GPU process'),
  blocked(
    'Page.crash',
    'deliberately crashes the page process',
    'Use goto or reload to reset a page.'
  ),
  blocked(
    'Page.close',
    'closes the page without releasing engine state',
    'Use --close-tab, or the cleanup command for a browser the engine opened.'
  ),
  blocked(
    'Security.setIgnoreCertificateErrors',
    'turns off certificate validation for every page',
    'Fix the certificate, or open the browser with the documented launch option.'
  ),
  blocked(
    'Target.setRemoteLocations',
    'exposes the browser to remote debugging locations',
    'Connect to the loopback port only.'
  ),
  blocked(
    'Target.exposeDevToolsProtocol',
    'hands the DevTools protocol to page scripts',
    'Send the method from the engine instead.'
  ),
  needsAck(
    'Browser.grantPermissions',
    'grants site permissions (camera, location, clipboard...)'
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
  needsAck('WebAuthn.addVirtualAuthenticator', 'adds a fake authenticator'),
  needsAck('WebAuthn.addCredential', 'stores a credential'),
  needsAck('WebAuthn.getCredentials', 'reads stored credentials'),
  needsAck('Fetch.fulfillRequest', 'answers a request with a made-up reply'),
  needsAck('Fetch.continueRequest', 'modifies traffic in flight'),
  needsAck('Fetch.failRequest', 'drops traffic in flight'),
];

const byMethod = new Map(CDP_POLICY_TABLE.map(entry => [entry.method, entry]));

export const policyEntry = (method: string): PolicyEntry | undefined =>
  byMethod.get(method);

/** The policy of one method; anything not in the table is free. */
export const classify = (method: string): CdpPolicy =>
  byMethod.get(method)?.policy ?? 'free';

/** The refusal text for a blocked method; undefined when the method is not blocked. */
export function blockedRefusal(method: string): string | undefined {
  const entry = byMethod.get(method);
  if (entry?.policy !== 'blocked') return undefined;
  return `[POLICY] ${method} is blocked: it ${entry.reason}. ${entry.alternative}`;
}

/** The refusal text for a method that needs the user's permission and has none; undefined otherwise. */
export function ackRefusal(
  method: string,
  acknowledge: boolean | undefined
): string | undefined {
  const entry = byMethod.get(method);
  if (entry?.policy !== 'needsAck' || acknowledge === true) return undefined;
  return `[POLICY] ${method} ${entry.reason} and needs the user's permission: ask them, then retry with acknowledge: true`;
}

/** Refusal for a blocked method, else for a missing acknowledgement, else undefined. */
export const policyRefusal = (
  method: string,
  acknowledge: boolean | undefined
) => blockedRefusal(method) ?? ackRefusal(method, acknowledge);
