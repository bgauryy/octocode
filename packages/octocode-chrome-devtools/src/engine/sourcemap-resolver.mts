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
type ParsedMap = {
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
      value |= (digit & 31) << shift;
      shift += 5;
    } while (digit & 32);
    result.push(value & 1 ? -(value >> 1) : value >> 1);
  }
  return result;
}

function parseMap(mapJson: SourceMapJson): ParsedMap {
  delete mapJson.sourcesContent;

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

function originalPositionFor(
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

async function fetchText(url: string, timeoutMs = 4000) {
  const { get } = url.startsWith('https')
    ? await import('https')
    : await import('http');
  return new Promise<string>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('timeout')), timeoutMs);
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
    }).on('error', e => {
      clearTimeout(timer);
      reject(e);
    });
  });
}

/** The map a script names: inline base64 data or a fetch relative to the script; null for inline non-base64 data. */
async function readMapText(sourceMapURL: string, url: string) {
  if (sourceMapURL.startsWith('data:')) {
    const b64start = sourceMapURL.indexOf('base64,');
    if (b64start === -1) return null;
    return Buffer.from(sourceMapURL.slice(b64start + 7), 'base64').toString(
      'utf8'
    );
  }
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
