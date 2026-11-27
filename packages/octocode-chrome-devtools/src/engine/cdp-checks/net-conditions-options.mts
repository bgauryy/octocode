// Option parsing and validation for the network-conditions recipe: everything is checked
// before the page is touched, so a bad option never leaves a half-applied condition.

export type Mock = {
  match: string;
  status: number;
  headers: Record<string, string>;
  body: string;
};

export type Conditions = {
  url: string;
  offline: boolean;
  latencyMs: number;
  downKbps: number;
  upKbps: number;
  blockUrls: string[];
  extraHeaders: Record<string, string>;
  mocks: Mock[];
};

const json = (name: string, raw: string | undefined): unknown => {
  if (raw === undefined || raw === '') return undefined;
  try {
    return JSON.parse(raw);
  } catch {
    throw new Error(`${name} must be valid JSON`);
  }
};

const number = (name: string, raw: string | undefined) => {
  if (raw === undefined || raw === '') return -1;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < 0)
    throw new Error(`${name} must be a non-negative number`);
  return value;
};

const stringMap = (name: string, value: unknown): Record<string, string> => {
  if (value === undefined) return {};
  if (
    typeof value !== 'object' ||
    value === null ||
    Array.isArray(value) ||
    !Object.values(value).every(v => typeof v === 'string')
  )
    throw new Error(`${name} must be a JSON object of string values`);
  return value as Record<string, string>;
};

/** The conditions from the recipe's environment; throws a plain message when one is invalid. */
export function parseConditions(env: NodeJS.ProcessEnv): Conditions {
  if (!env.NETCOND_URL) throw new Error('NETCOND_URL is required');
  const blockUrls = json('NETCOND_BLOCK_URLS', env.NETCOND_BLOCK_URLS) ?? [];
  if (
    !Array.isArray(blockUrls) ||
    !blockUrls.every(p => typeof p === 'string' && p)
  )
    throw new Error(
      'NETCOND_BLOCK_URLS must be a JSON array of non-empty URL patterns'
    );
  const rawMocks = json('NETCOND_MOCKS', env.NETCOND_MOCKS) ?? [];
  if (!Array.isArray(rawMocks))
    throw new Error('NETCOND_MOCKS must be a JSON array');
  const mocks = rawMocks.map((m, i): Mock => {
    const at = `NETCOND_MOCKS[${i}]`;
    if (typeof m !== 'object' || m === null)
      throw new Error(`${at} must be an object`);
    if (typeof m.match !== 'string' || !m.match)
      throw new Error(`${at}.match must be a non-empty string`);
    if (!Number.isInteger(m.status) || m.status < 100 || m.status > 599)
      throw new Error(`${at}.status must be an integer from 100 to 599`);
    if (m.body !== undefined && typeof m.body !== 'string')
      throw new Error(`${at}.body must be a string`);
    return {
      match: m.match,
      status: m.status,
      headers: stringMap(`${at}.headers`, m.headers),
      body: m.body ?? '',
    };
  });
  return {
    url: env.NETCOND_URL,
    offline: env.NETCOND_OFFLINE === '1',
    latencyMs: number('NETCOND_LATENCY_MS', env.NETCOND_LATENCY_MS),
    downKbps: number('NETCOND_DOWN_KBPS', env.NETCOND_DOWN_KBPS),
    upKbps: number('NETCOND_UP_KBPS', env.NETCOND_UP_KBPS),
    blockUrls,
    extraHeaders: stringMap(
      'NETCOND_EXTRA_HEADERS',
      json('NETCOND_EXTRA_HEADERS', env.NETCOND_EXTRA_HEADERS)
    ),
    mocks,
  };
}

/** Kilobits per second as the bytes per second CDP takes; -1 (unset) stays unthrottled. */
export const kbpsToBytes = (kbps: number) =>
  kbps < 0 ? -1 : (kbps * 1000) / 8;

/** A request URL against a mock's `match`: a glob when it holds `*`, else a substring. */
export function urlMatches(url: string, match: string): boolean {
  if (!match.includes('*')) return url.includes(match);
  const source = match
    .split('*')
    .map(part => part.replace(/[.+?^${}()|[\]\\]/g, '\\$&'))
    .join('.*');
  return new RegExp(`^${source}$`).test(url);
}
