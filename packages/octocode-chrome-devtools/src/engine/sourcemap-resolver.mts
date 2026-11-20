import type { Session } from './cdp-connection.mjs';

/** Source map v3 fields the resolver reads; sourcesContent is dropped on load. */
type SourceMapJson = {
  mappings: string;
  sources?: (string | null)[];
  names?: string[];
  sourcesContent?: unknown;
};
/** One mapping: generated line/column to source index, line, column and name index (-1: none). */
type Segment = {
  gl: number;
  gc: number;
  si: number;
  sl: number;
  sc: number;
  ni: number;
};
/** `Debugger.scriptParsed` event params the resolver reads. */
type ScriptParsed = { scriptId: string; url: string; sourceMapURL?: string };
export type ParsedMap = {
  segments: Segment[];
  sources: (string | null)[];
  names: string[];
};

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const B64_MAP = new Uint8Array(256).fill(255);
for (let i = 0; i < B64.length; i++) B64_MAP[B64.charCodeAt(i)] = i;

function decodeVLQList(str: string) {
  const result: number[] = [];
  let i = 0;
  while (i < str.length) {
    let value = 0,
      shift = 0,
      digit;
    do {
      digit = B64_MAP[str.charCodeAt(i++)];
      if (digit === undefined || digit === 255)
        throw new Error('Malformed source map: invalid VLQ character');
      value |= (digit & 31) << shift;
      shift += 5;
    } while (digit & 32);
    result.push(value & 1 ? -(value >> 1) : value >> 1);
  }
  return result;
}

function parseMap(mapJson: SourceMapJson): ParsedMap {
  const sources = mapJson.sources ?? [];
  const names = mapJson.names ?? [];
  const segments: Segment[] = [];

  let genLine = 0;
  let srcIdx = 0,
    srcLine = 0,
    srcCol = 0,
    nameIdx = 0;

  for (const lineStr of mapJson.mappings.split(';')) {
    let genCol = 0;
    if (lineStr) {
      for (const segStr of lineStr.split(',')) {
        if (!segStr) continue;
        const fields = decodeVLQList(segStr);
        genCol += fields[0];
        if (fields.length >= 4) {
          srcIdx += fields[1];
          srcLine += fields[2];
          srcCol += fields[3];
          const ni = fields.length >= 5 ? nameIdx + fields[4] : -1;
          if (fields.length >= 5) nameIdx += fields[4];
          segments.push({
            gl: genLine,
            gc: genCol,
            si: srcIdx,
            sl: srcLine,
            sc: srcCol,
            ni,
          });
        }
      }
    }
    genLine++;
  }

  segments.sort((a, b) => (a.gl !== b.gl ? a.gl - b.gl : a.gc - b.gc));

  return { segments, sources, names };
}

/** Original position for a 0-based generated line and column: source, 1-based line, 0-based column, name. */
export function originalPositionFor(
  parsed: ParsedMap,
  genLine: number,
  genCol: number
) {
  const { segments, sources, names } = parsed;
  if (!segments.length) return null;

  let lo = 0,
    hi = segments.length - 1,
    best = -1;

  while (lo <= hi) {
    const mid = (lo + hi) >>> 1;
    const s = segments[mid];
    if (s.gl < genLine || (s.gl === genLine && s.gc <= genCol)) {
      if (s.gl === genLine) best = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }

  if (best === -1) return null;
  const seg = segments[best];

  return {
    source: sources[seg.si] ?? null,
    line: seg.sl + 1,
    col: seg.sc,
    name: seg.ni >= 0 ? (names[seg.ni] ?? null) : null,
  };
}

/** Runs fn inside the guard's narrow network exemption when the sandbox guard is installed. */
function withSandboxExemption<T>(fn: () => T): T {
  const scope = (globalThis as Record<symbol, unknown>)[
    Symbol.for('octocode.chrome-devtools.sandbox-exempt')
  ] as { run<R>(store: boolean, f: () => R): R } | undefined;
  return scope ? scope.run(true, fn) : fn();
}

async function fetchText(url: string, timeoutMs = 4000) {
  const { get } = url.startsWith('https')
    ? await import('https')
    : await import('http');
  return new Promise<string>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('timeout')), timeoutMs);
    withSandboxExemption(() =>
      get(url, res => {
        if (res.statusCode !== 200) {
          clearTimeout(timer);
          res.resume();
          return reject(new Error(`HTTP ${res.statusCode}`));
        }
        const chunks: Buffer[] = [];
        res.on('data', c => chunks.push(c));
        res.on('end', () => {
          clearTimeout(timer);
          resolve(Buffer.concat(chunks).toString('utf8'));
        });
        res.on('error', e => {
          clearTimeout(timer);
          reject(e);
        });
      })
    ).on('error', e => {
      clearTimeout(timer);
      reject(e);
    });
  });
}

/** Source map text to a decoder; throws a clear error for invalid JSON, missing mappings or an indexed map. */
export function parseSourceMap(text: string): ParsedMap {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch {
    throw new Error('Malformed source map: not valid JSON');
  }
  const map = json as SourceMapJson & { sections?: unknown };
  if (!map || typeof map !== 'object')
    throw new Error('Malformed source map: not an object');
  if (map.sections !== undefined)
    throw new Error('Malformed source map: indexed maps are not supported');
  if (typeof map.mappings !== 'string' || !Array.isArray(map.sources))
    throw new Error('Malformed source map: mappings or sources missing');
  return parseMap(map);
}

/** The origin a URL is judged by: scheme and host with port. */
function originOf(url: string) {
  const u = new URL(url);
  return `${u.protocol}//${u.host}`;
}

/**
 * A script's map text: a data: URI decodes locally; an http(s) map is fetched through the sandbox
 * exemption only when it shares the script's origin. A foreign origin, or a relative URL with no
 * script URL, is refused so the recipe never widens network access.
 */
export async function readScriptMapText(
  sourceMapURL: string,
  scriptUrl: string
): Promise<string> {
  if (sourceMapURL.startsWith('data:')) {
    const text = decodeDataUri(sourceMapURL);
    if (text === null) throw new Error('Malformed source map: bad data: URI');
    return text;
  }
  if (!scriptUrl)
    throw new Error('Refused: a relative source map needs a script URL');
  const mapUrl = new URL(sourceMapURL, scriptUrl).href;
  if (!/^https?:/.test(mapUrl))
    throw new Error(`Refused: unsupported source map scheme ${mapUrl}`);
  if (originOf(mapUrl) !== originOf(scriptUrl))
    throw new Error(
      `Refused: source map origin ${originOf(mapUrl)} differs from script origin ${originOf(scriptUrl)}`
    );
  return fetchText(mapUrl);
}

/** The text of a data: URI (base64 or percent-encoded); null when malformed. */
function decodeDataUri(sourceMapURL: string): string | null {
  const b64start = sourceMapURL.indexOf('base64,');
  if (b64start !== -1)
    return Buffer.from(sourceMapURL.slice(b64start + 7), 'base64').toString(
      'utf8'
    );
  const comma = sourceMapURL.indexOf(',');
  if (comma === -1) return null;
  try {
    return decodeURIComponent(sourceMapURL.slice(comma + 1));
  } catch {
    return null;
  }
}

/** The map a script names: inline data or a fetch relative to the script; null for malformed inline data. */
function readMapText(sourceMapURL: string, url: string) {
  if (sourceMapURL.startsWith('data:')) return decodeDataUri(sourceMapURL);
  return fetchText(url ? new URL(sourceMapURL, url).href : sourceMapURL);
}

export async function createSourceMapResolver(cdp: Session) {
  await cdp.send('Debugger.enable', {});
  await cdp.send('Debugger.setSkipAllPauses', { skip: true });

  const parsedMaps = new Map<string, ParsedMap | null>();

  const stats = { withMap: 0, withoutMap: 0, loaded: 0, failed: 0 };

  const loadPromises: Promise<void>[] = [];

  cdp.on('Debugger.scriptParsed', (event: ScriptParsed) => {
    const { scriptId, url, sourceMapURL } = event;
    if (!sourceMapURL) {
      stats.withoutMap++;
      return;
    }

    stats.withMap++;

    const loadMap = async () => {
      try {
        const mapText = await readMapText(sourceMapURL, url);
        if (mapText === null) return;
        const mapJson = JSON.parse(mapText);
        const parsed = parseMap(mapJson);
        parsedMaps.set(scriptId, parsed);
        stats.loaded++;
      } catch {
        stats.failed++;
        parsedMaps.set(scriptId, null);
      }
    };

    loadPromises.push(loadMap());
  });

  return {
    async settle(timeoutMs = 8000) {
      let timer: NodeJS.Timeout | undefined;
      try {
        await Promise.race([
          Promise.allSettled(loadPromises),
          new Promise(r => {
            timer = setTimeout(r, timeoutMs);
          }),
        ]);
      } finally {
        if (timer) clearTimeout(timer);
      }
    },

    resolve(scriptId: string, line: number, col: number) {
      const parsed = parsedMaps.get(scriptId);
      if (!parsed) return null;
      return originalPositionFor(parsed, line, col);
    },

    hasMap(scriptId: string) {
      const m = parsedMaps.get(scriptId);
      return m !== null && m !== undefined;
    },

    printSummary() {
      const total = stats.withMap + stats.withoutMap;
      console.log(
        `[SOURCEMAP] ${total} scripts: ` +
          `${stats.loaded} maps loaded, ` +
          `${stats.failed} failed, ` +
          `${stats.withoutMap} had no map`
      );
    },
  };
}
