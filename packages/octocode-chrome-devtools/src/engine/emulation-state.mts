// Browser emulation set by `open --emulation <json>`: parsed and checked here against the field
// guide the contract publishes (names and kinds); the contract's own schema owns the ranges.
// The validated object is stored in the port's session meta and re-sent by the runner on every connection.
import { chromeEmulationGuide } from './chrome-contract.mjs';
import { readSessionMeta } from './session-meta.mjs';

export type Emulation = Record<string, unknown>;
const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** Whether a value has the kind a guide entry names: number, boolean, string, an enum or an object. */
function hasKind(value: unknown, kind: string): boolean {
  if (kind.startsWith('"'))
    return kind.split('|').some(one => one === JSON.stringify(value));
  if (kind.startsWith('{')) return isRecord(value);
  return typeof value === kind && (kind !== 'number' || Number.isFinite(value));
}

/** The emulation of an `open` call; throws a one-line refusal for an unknown key, a wrong kind or a missing consent. */
export function parseEmulation(
  text: string,
  options: { acknowledged: boolean }
): Emulation {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error('--emulation needs a JSON object');
  }
  if (!isRecord(value)) throw new Error('--emulation needs a JSON object');
  const fields: Record<string, { type: string }> = chromeEmulationGuide.fields;
  for (const [key, entry] of Object.entries(value)) {
    const field = Object.hasOwn(fields, key) ? fields[key] : undefined;
    if (!field)
      throw new Error(
        `--emulation: unknown field ${key}; allowed: ${Object.keys(fields).join(', ')}`
      );
    if (!hasKind(entry, field.type))
      throw new Error(`--emulation: ${key} must be ${field.type}`);
  }
  if (value.reset === true && Object.keys(value).length > 1)
    throw new Error('--emulation: reset cannot combine with other keys');
  if (value.geolocation !== undefined && !options.acknowledged)
    throw new Error(
      '--emulation geolocation requires --acknowledge: it spoofs the browser location, only with the user consent'
    );
  checkRanges(value);
  return value;
}

const num = (value: Record<string, unknown>, key: string) =>
  typeof value[key] === 'number' ? value[key] : undefined;

/** Numeric bounds the field guide does not carry; throws a one-line refusal. */
function checkRanges(value: Record<string, unknown>) {
  for (const key of ['width', 'height']) {
    const n = num(value, key);
    if (n !== undefined && !(Number.isInteger(n) && n > 0))
      throw new Error(`--emulation: ${key} must be a positive integer`);
  }
  const scale = num(value, 'deviceScaleFactor');
  if (scale !== undefined && !(scale > 0))
    throw new Error('--emulation: deviceScaleFactor must be greater than 0');
  const cpu = num(value, 'cpuThrottle');
  if (cpu !== undefined && !(cpu >= 1))
    throw new Error('--emulation: cpuThrottle must be at least 1');
  const geo = value.geolocation;
  if (isRecord(geo)) {
    const inRange = (key: string, min: number, max: number) => {
      const n = geo[key];
      if (typeof n !== 'number' || !(n >= min && n <= max))
        throw new Error(
          `--emulation: geolocation.${key} must be a number from ${min} to ${max}`
        );
    };
    inRange('latitude', -90, 90);
    inRange('longitude', -180, 180);
    if (
      geo.accuracy !== undefined &&
      !(typeof geo.accuracy === 'number' && geo.accuracy >= 0)
    )
      throw new Error(
        '--emulation: geolocation.accuracy must be a number of at least 0'
      );
  }
}

/** The open reply for a validated emulation: key names only, never the coordinates. */
export function emulationReply(emulation: Emulation) {
  if (emulation.reset === true) return { reset: true };
  return {
    applied: Object.keys(emulation).map(key =>
      key === 'geolocation' ? 'geolocation: set' : key
    ),
    persistent: true,
  };
}

type Sender = {
  send: (method: string, params?: object) => Promise<unknown>;
  targetInfo?: { type?: string };
};

/** The CDP calls one stored emulation needs, as [method, params]. */
export function emulationCalls(
  e: Emulation,
  currentUserAgent?: string
): [string, object][] {
  const calls: [string, object][] = [];
  if (e.width !== undefined || e.height !== undefined)
    calls.push([
      'Emulation.setDeviceMetricsOverride',
      {
        width: e.width ?? 0,
        height: e.height ?? 0,
        deviceScaleFactor: e.deviceScaleFactor ?? 1,
        mobile: e.mobile ?? false,
      },
    ]);
  // navigator.language follows the Accept-Language of the user-agent override, not the locale override alone.
  const userAgent =
    e.userAgent ?? (e.locale !== undefined ? currentUserAgent : undefined);
  if (typeof userAgent === 'string')
    calls.push([
      'Emulation.setUserAgentOverride',
      {
        userAgent,
        ...(e.locale !== undefined ? { acceptLanguage: e.locale } : {}),
      },
    ]);
  const features = [
    e.colorScheme !== undefined && {
      name: 'prefers-color-scheme',
      value: e.colorScheme,
    },
    e.reducedMotion !== undefined && {
      name: 'prefers-reduced-motion',
      value: e.reducedMotion ? 'reduce' : 'no-preference',
    },
  ].filter(Boolean);
  if (features.length) calls.push(['Emulation.setEmulatedMedia', { features }]);
  if (e.timezone !== undefined)
    calls.push(['Emulation.setTimezoneOverride', { timezoneId: e.timezone }]);
  if (e.locale !== undefined)
    calls.push(['Emulation.setLocaleOverride', { locale: e.locale }]);
  if (isRecord(e.geolocation))
    calls.push([
      'Emulation.setGeolocationOverride',
      {
        latitude: e.geolocation.latitude,
        longitude: e.geolocation.longitude,
        accuracy: e.geolocation.accuracy ?? 1,
      },
    ]);
  if (e.cpuThrottle !== undefined)
    calls.push(['Emulation.setCPUThrottlingRate', { rate: e.cpuThrottle }]);
  if (e.touch !== undefined)
    calls.push(['Emulation.setTouchEmulationEnabled', { enabled: e.touch }]);
  return calls;
}

/**
 * Sends the port's stored emulation to the connected target. Costs nothing when none is stored.
 * A failed call is a [WARNING] line and never stops the run; a browser-level target gets a note.
 */
export async function applyStoredEmulation(
  cdp: Sender,
  port: string | number,
  workspace: string,
  log: (line: string) => void = line => process.stderr.write(`${line}\n`)
): Promise<void> {
  const stored = readSessionMeta(port, workspace).emulation;
  if (!stored) return;
  if (cdp.targetInfo?.type === 'browser') {
    log(
      '[EMULATION] stored emulation skipped: browser-level target has no page'
    );
    return;
  }
  let currentUserAgent: string | undefined;
  if (stored.locale !== undefined && stored.userAgent === undefined)
    try {
      const probe = (await cdp.send('Runtime.evaluate', {
        expression: 'navigator.userAgent',
        returnByValue: true,
      })) as { result?: { value?: unknown } };
      if (typeof probe.result?.value === 'string')
        currentUserAgent = probe.result.value;
    } catch {
      // Best effort: without the current UA the locale override still applies, only acceptLanguage is skipped.
    }
  for (const [method, params] of emulationCalls(stored, currentUserAgent)) {
    try {
      await cdp.send(method, params);
    } catch (error) {
      log(
        `[WARNING] ${method} failed: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }
}
