/**
 * Cookie mapping between CDP (Network/Storage.getCookies, setCookies) and the
 * Playwright storage-state file format. Every field that changes where a cookie is
 * sent survives a transfer: host-only vs domain scope, path, Secure, HttpOnly,
 * SameSite, expiry, CHIPS partition, and source scheme/port.
 */

type CdpPartitionKey = { topLevelSite: string; hasCrossSiteAncestor: boolean };

/** A cookie as CDP reports it or a storage-state file stores it; any field but the name may be missing. */
export type CookieRecord = {
  name: string;
  value?: string;
  domain?: string;
  path?: string;
  url?: string;
  secure?: boolean;
  httpOnly?: boolean;
  sameSite?: string;
  session?: boolean;
  expires?: number | null;
  priority?: string;
  sourceScheme?: string;
  sourcePort?: number;
  partitionKey?:
    string | { topLevelSite: string; hasCrossSiteAncestor?: boolean };
  _crHasCrossSiteAncestor?: boolean;
};

/** CookieParam for Storage.setCookies / Network.setCookies. */
export type CookieParam = {
  name: string;
  value?: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  domain?: string;
  url?: string;
  expires?: number;
  sameSite?: string;
  priority?: string;
  sourceScheme?: string;
  sourcePort?: number;
  partitionKey?: CdpPartitionKey;
};

/** A Playwright storage-state cookie. */
export type StorageStateCookie = {
  name: string;
  value?: string;
  domain: string;
  path: string;
  expires: number;
  httpOnly: boolean;
  secure: boolean;
  sameSite: string;
  partitionKey?: string;
  _crHasCrossSiteAncestor?: boolean;
};

/** Value-free description used for logs and findings. */
export function cookieMeta(c: CookieRecord) {
  return {
    name: c.name,
    domain: c.domain || '',
    path: c.path || '/',
    secure: Boolean(c.secure),
    httpOnly: Boolean(c.httpOnly),
    sameSite: c.sameSite || 'Lax',
    session: c.session ?? (c.expires === -1 || c.expires === undefined),
    partitioned: Boolean(c.partitionKey),
  };
}

/** Playwright stores the partition as its top-level site string. */
function partitionKeyOf(c: CookieRecord): CdpPartitionKey | undefined {
  const key = c.partitionKey;
  if (!key) return undefined;
  if (typeof key === 'string')
    return {
      topLevelSite: key,
      hasCrossSiteAncestor: c._crHasCrossSiteAncestor ?? true,
    };
  return {
    topLevelSite: key.topLevelSite,
    hasCrossSiteAncestor: Boolean(key.hasCrossSiteAncestor),
  };
}

/**
 * CookieParam for Storage.setCookies / Network.setCookies. A domain without a leading dot stays host-only;
 * `urls[0]` only fills a cookie that has neither url nor domain.
 */
export function toSetCookie(c: CookieRecord, urls: string[] = []) {
  const out: CookieParam = {
    name: c.name,
    value: c.value,
    path: c.path || '/',
    secure: Boolean(c.secure),
    httpOnly: Boolean(c.httpOnly),
  };
  if (c.domain) out.domain = c.domain;
  if (c.url) out.url = c.url;
  else if (!c.domain && urls[0]) out.url = urls[0];
  if (c.expires != null && c.expires !== -1 && !c.session)
    out.expires = c.expires;
  if (c.sameSite) out.sameSite = c.sameSite;
  if (c.priority) out.priority = c.priority;
  if (c.sourceScheme) out.sourceScheme = c.sourceScheme;
  if (Number.isInteger(c.sourcePort)) out.sourcePort = c.sourcePort;
  const partitionKey = partitionKeyOf(c);
  if (partitionKey) out.partitionKey = partitionKey;
  return out;
}

/** Playwright storage-state cookie (same defaults as Playwright for Chromium). */
export function toStorageStateCookie(c: CookieRecord) {
  const out: StorageStateCookie = {
    name: c.name,
    value: c.value,
    domain: c.domain || '',
    path: c.path || '/',
    expires: c.session ? -1 : (c.expires ?? -1),
    httpOnly: Boolean(c.httpOnly),
    secure: Boolean(c.secure),
    sameSite:
      c.sameSite === 'None' || c.sameSite === 'Strict' || c.sameSite === 'Lax'
        ? c.sameSite
        : 'Lax',
  };
  const partitionKey = partitionKeyOf(c);
  if (partitionKey) {
    out.partitionKey = partitionKey.topLevelSite;
    out._crHasCrossSiteAncestor = partitionKey.hasCrossSiteAncestor;
  }
  return out;
}

/** RFC 6265 domain match: host-only cookies match their exact host. */
function cookieMatchesHost(c: CookieRecord, host: string) {
  const domain = (c.domain || '').toLowerCase();
  if (!domain) return false;
  host = host.toLowerCase();
  if (!domain.startsWith('.')) return host === domain;
  const base = domain.slice(1);
  return host === base || host.endsWith(domain);
}

export function filterCookies(cookies: CookieRecord[], urls: string[] = []) {
  if (!urls.length) return cookies;
  const hosts = urls.flatMap(u => {
    try {
      return [new URL(u).hostname];
    } catch {
      return [];
    }
  });
  return cookies.filter(c => hosts.some(host => cookieMatchesHost(c, host)));
}

export function readStorageStateCookies(raw: unknown): CookieRecord[] {
  const list = Array.isArray(raw) ? raw : cookiesField(raw);
  if (!Array.isArray(list))
    throw new Error('storage state must be {cookies:[...]} or an array');
  return list;
}

function cookiesField(raw: unknown) {
  return typeof raw === 'object' && raw !== null && 'cookies' in raw
    ? raw.cookies
    : undefined;
}

/** Identity of a stored cookie: name, scope and partition. */
export function cookieKey(c: CookieRecord) {
  const partition = partitionKeyOf(c);
  let domain = c.domain || '';
  if (!domain && c.url)
    try {
      domain = new URL(c.url).hostname;
    } catch {}
  return [
    c.name,
    domain.toLowerCase(),
    c.path || '/',
    partition
      ? `${partition.topLevelSite}|${partition.hasCrossSiteAncestor}`
      : '',
  ].join('\u0000');
}
