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
type ScriptParsed = {
  scriptId: string;
  url: string;
  sourceMapURL?: string;
  executionContextId?: number;
  executionAuxData?: { frameId?: string };
};
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

const MAP_BYTES_PER_READ = 1 << 20;

/** The frame a map load is made for: the given one, else the page's main frame. */
async function frameIdOf(cdp: Session, frameId?: string) {
  if (frameId) return frameId;
  const tree = await cdp.send('Page.getFrameTree');
  const id = tree?.frameTree?.frame?.id;
  if (typeof id !== 'string')
    throw new Error('Source map load needs a page frame');
  return id;
}

/** Loads a URL with Chrome's own network stack (cookies, cache, proxy) and reads the whole body. */
async function loadNetworkText(cdp: Session, url: string, frameId?: string) {
  let loaded;
  try {
    loaded = await cdp.send('Network.loadNetworkResource', {
      frameId: await frameIdOf(cdp, frameId),
      url,
      options: { disableCache: false, includeCredentials: true },
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (/wasn.t found|not found|not supported/i.test(message))
      throw new Error(
        `Network.loadNetworkResource is unavailable in this Chrome (${message}); source maps cannot be loaded`,
        { cause: error }
      );
    throw error;
  }
  const resource = loaded?.resource;
  if (!resource?.success || !resource.stream) {
    const why = [
      resource?.netError !== undefined
        ? `netError ${resource.netError}${resource.netErrorName ? ` ${resource.netErrorName}` : ''}`
        : null,
      resource?.httpStatusCode !== undefined
        ? `HTTP ${resource.httpStatusCode}`
        : null,
    ].filter(Boolean);
    throw new Error(`Source map load failed: ${why.join(', ') || 'no stream'}`);
  }
  const chunks: Buffer[] = [];
  try {
    for (;;) {
      const part = await cdp.send('IO.read', {
        handle: resource.stream,
        size: MAP_BYTES_PER_READ,
      });
      chunks.push(
        Buffer.from(part.data ?? '', part.base64Encoded ? 'base64' : 'utf8')
      );
      if (part.eof) break;
    }
  } finally {
    await cdp.send('IO.close', { handle: resource.stream }).catch(() => {});
  }
  return Buffer.concat(chunks).toString('utf8');
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
 * A script's map text: a data: URI decodes locally; an http(s) map loads through the page's Chrome
 * (Network.loadNetworkResource) only when it shares the script's origin. A foreign origin, or a relative URL with no
 * script URL, is refused so the recipe never widens network access.
 */
export async function readScriptMapText(
  cdp: Session,
  sourceMapURL: string,
  scriptUrl: string,
  frameId?: string
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
  return loadNetworkText(cdp, mapUrl, frameId);
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
        const mapText = await readScriptMapText(
          cdp,
          sourceMapURL,
          url,
          event.executionContextId === undefined
            ? undefined
            : event.executionAuxData?.frameId
        );
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
