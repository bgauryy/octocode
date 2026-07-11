import crypto from 'crypto';

// Bump when cache-key shape changes (auth fingerprint, new fields) so stale
// entries from older key schemas cannot be served.
const VERSION = 'v2';

const CACHE_KEY_EXCLUDED_FIELDS: ReadonlySet<string> = new Set([]);

function stripCacheKeyExcludedFields(params: unknown): unknown {
  if (params === null || typeof params !== 'object' || Array.isArray(params)) {
    return params;
  }
  const obj = params as Record<string, unknown>;
  let touched = false;
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(obj)) {
    if (CACHE_KEY_EXCLUDED_FIELDS.has(k)) {
      touched = true;
      continue;
    }
    out[k] = obj[k];
  }
  return touched ? out : params;
}

export function generateCacheKey(
  prefix: string,
  params: unknown,
  sessionId?: string
): string {
  const paramString = createStableParamString(
    stripCacheKeyExcludedFields(params)
  );

  const finalParamString = sessionId
    ? `${sessionId}:${paramString}`
    : paramString;

  const hash = crypto
    .createHash('sha256')
    .update(finalParamString)
    .digest('hex');

  return `${VERSION}-${prefix}:${hash}`;
}

function createStableParamString(
  params: unknown,
  visited: WeakSet<object> = new WeakSet()
): string {
  if (params === null) {
    return 'null';
  }

  if (params === undefined) {
    return 'undefined';
  }

  if (typeof params !== 'object') {
    return String(params);
  }

  if (visited.has(params as object)) {
    return '"[Circular]"';
  }
  visited.add(params as object);

  if (Array.isArray(params)) {
    return `[${params.map(p => createStableParamString(p, visited)).join(',')}]`;
  }

  const sortedKeys = Object.keys(params as Record<string, unknown>).sort();
  const sortedEntries = sortedKeys.map(key => {
    const value = (params as Record<string, unknown>)[key];
    return `"${key}":${createStableParamString(value, visited)}`;
  });

  return `{${sortedEntries.join(',')}}`;
}
