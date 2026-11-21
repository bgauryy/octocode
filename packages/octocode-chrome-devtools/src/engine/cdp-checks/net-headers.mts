// Header redaction shared by the network recipes: names stay, credential values do not.

/** Names broader than the exact set below that still carry credentials (api keys, tokens, secrets). */
export const CREDENTIAL_NAME =
  /cookie|authorization|proxy-authorization|x-api-key|token|secret/i;

const SENSITIVE = new Set([
  'authorization',
  'cookie',
  'set-cookie',
  'proxy-authorization',
]);

/** Headers with every credential value replaced by `[redacted N chars]`; other values are kept. */
export function redactHeaders(
  headers: Record<string, unknown> | undefined
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [name, raw] of Object.entries(headers ?? {})) {
    const value = String(raw);
    out[name] = SENSITIVE.has(name.toLowerCase())
      ? `[redacted ${value.length} chars]`
      : value;
  }
  return out;
}

/** Headers of one message: the ordinary event's, plus those only the network stack's extra-info event carries (Cookie, Set-Cookie). */
export function mergeHeaders(
  ...sources: (Record<string, unknown> | undefined)[]
): Record<string, string> {
  const merged = new Map<string, [string, string]>();
  for (const source of sources)
    for (const [name, value] of Object.entries(source ?? {}))
      if (!merged.has(name.toLowerCase()))
        merged.set(name.toLowerCase(), [name, String(value)]);
  return Object.fromEntries(merged.values());
}

/** One cookie a request carried or a response set: identity and why it was blocked; never its value. */
export type CookieRow = {
  direction: 'request' | 'response';
  name: string;
  domain: string | null;
  path: string | null;
  blockedReasons: string[];
};

type CdpCookie = { name?: unknown; domain?: unknown; path?: unknown };
type CdpCookieEntry = {
  blockedReasons?: unknown;
  cookie?: CdpCookie;
  cookieLine?: unknown;
};

const text = (value: unknown) => (typeof value === 'string' ? value : null);
const reasons = (value: unknown) =>
  Array.isArray(value) ? value.map(String) : [];

/** The name (before the first `=`) of one `name=value` pair, or null; the value is dropped here. */
const pairName = (pair: string) => {
  const at = pair.indexOf('=');
  const name = (at < 0 ? '' : pair.slice(0, at)).trim();
  return name || null;
};

/** The cookies of `associatedCookies` / `blockedCookies`: name, domain, path and reasons only. */
export function cookieRows(
  direction: CookieRow['direction'],
  entries: unknown
): CookieRow[] {
  const rows: CookieRow[] = [];
  for (const entry of Array.isArray(entries)
    ? (entries as CdpCookieEntry[])
    : []) {
    const cookie = entry?.cookie;
    // A cookie Chrome could not parse has only its line; the name is the part before `=`.
    const name = text(cookie?.name) ?? pairName(text(entry?.cookieLine) ?? '');
    if (name)
      rows.push({
        direction,
        name,
        domain: text(cookie?.domain),
        path: text(cookie?.path),
        blockedReasons: reasons(entry?.blockedReasons),
      });
  }
  return rows;
}

const headerValue = (headers: Record<string, string>, lower: string) =>
  Object.entries(headers).find(([name]) => name.toLowerCase() === lower)?.[1];

/** Cookies a message's own headers name: `Cookie: a=1; b=2` and `Set-Cookie: a=1; Path=/` lines (value dropped). */
export function headerCookieRows(
  direction: CookieRow['direction'],
  headers: Record<string, string>
): CookieRow[] {
  if (direction === 'request')
    return (headerValue(headers, 'cookie')?.split(';') ?? []).flatMap(pair => {
      const name = pairName(pair);
      return name
        ? [{ direction, name, domain: null, path: null, blockedReasons: [] }]
        : [];
    });
  return (headerValue(headers, 'set-cookie')?.split('\n') ?? []).flatMap(
    line => {
      const [pair = '', ...attributes] = line.split(';');
      const name = pairName(pair);
      const attribute = (key: string) =>
        attributes
          .map(part => part.trim())
          .find(part => part.toLowerCase().startsWith(`${key}=`))
          ?.slice(key.length + 1) ?? null;
      return name
        ? [
            {
              direction,
              name,
              domain: attribute('domain'),
              path: attribute('path'),
              blockedReasons: [],
            },
          ]
        : [];
    }
  );
}

/** A HAR 1.2 cookie: identity and attributes; the value is always redacted. */
export type HarCookie = {
  name: string;
  value: string;
  path?: string;
  domain?: string;
  expires?: string;
  httpOnly?: boolean;
  secure?: boolean;
  sameSite?: string;
};

const redactedCookie = (value: string) => `[redacted ${value.length} chars]`;

/**
 * The HAR-standard `cookies` array of one message, from its own headers: `Cookie: a=1; b=2` for a
 * request, one `Set-Cookie` line per cookie for a response (attributes kept). Values never leave here.
 */
export function harCookies(
  direction: CookieRow['direction'],
  headers: Record<string, string>
): HarCookie[] {
  if (direction === 'request')
    return (headerValue(headers, 'cookie')?.split(';') ?? []).flatMap(pair => {
      const name = pairName(pair);
      return name
        ? [
            {
              name,
              value: redactedCookie(pair.slice(pair.indexOf('=') + 1).trim()),
            },
          ]
        : [];
    });
  return (headerValue(headers, 'set-cookie')?.split('\n') ?? []).flatMap(
    line => {
      const [pair = '', ...attributes] = line.split(';');
      const name = pairName(pair);
      if (!name) return [];
      const cookie: HarCookie = {
        name,
        value: redactedCookie(pair.slice(pair.indexOf('=') + 1).trim()),
      };
      for (const part of attributes) {
        const at = part.indexOf('=');
        const key = (at < 0 ? part : part.slice(0, at)).trim().toLowerCase();
        const value = at < 0 ? '' : part.slice(at + 1).trim();
        if (key === 'path') cookie.path = value;
        else if (key === 'domain') cookie.domain = value;
        else if (key === 'expires') cookie.expires = value;
        else if (key === 'samesite') cookie.sameSite = value;
        else if (key === 'httponly') cookie.httpOnly = true;
        else if (key === 'secure') cookie.secure = true;
      }
      return [cookie];
    }
  );
}
