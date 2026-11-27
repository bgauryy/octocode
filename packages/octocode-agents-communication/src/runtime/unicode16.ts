/**
 * Frozen Unicode 16 canonical caseless keys, independent of the Node and ICU versions.
 *
 * Data: UnicodeData.txt and CaseFolding.txt from unicode.org/Public/16.0.0/ucd/. Source SHA-256 digests are in
 * unicode16.json; terms are in assets/UNICODE-LICENSE.txt. Changing these tables changes persistent
 * lease identity and needs a coordinated upgrade. Never use String#normalize or toLowerCase for keys.
 */
import tables from '../../assets/unicode16.json';

interface Tables {
  decomposition: Map<number, number[]>;
  combining: Map<number, number>;
  casefold: Map<number, number[]>;
}

let loaded: Tables | undefined;

function table<T>(source: Record<string, T>): Map<number, T> {
  return new Map(Object.entries(source).map(([key, value]) => [Number(key), value]));
}

function load(): Tables {
  loaded ??= { decomposition: table(tables.decomposition), combining: table(tables.combining), casefold: table(tables.casefold) };
  return loaded;
}

function nfd(points: Iterable<number>, { decomposition, combining }: Tables): number[] {
  const expanded: number[] = [];
  const expand = (point: number): void => {
    // Unicode's algorithmic Hangul decomposition is absent from UnicodeData.
    const index = point - 0xac00;
    const children = decomposition.get(point);
    if (index >= 0 && index < 11172) {
      expanded.push(0x1100 + Math.floor(index / 588), 0x1161 + Math.floor((index % 588) / 28));
      if (index % 28) expanded.push(0x11a7 + (index % 28));
    } else if (children) for (const child of children) expand(child);
    else expanded.push(point);
  };
  for (const point of points) expand(point);
  const result: number[] = [];
  let marks: number[] = [];
  const flush = (): void => {
    marks.sort((a, b) => combining.get(a)! - combining.get(b)!);
    result.push(...marks);
    marks = [];
  };
  for (const point of expanded) {
    if (!combining.get(point)) {
      flush();
      result.push(point);
    } else marks.push(point);
  }
  flush();
  return result;
}

function codePoints(value: string): number[] {
  return Array.from(value, character => character.codePointAt(0)!);
}

function fromPoints(points: number[]): string {
  let result = '';
  for (let start = 0; start < points.length; start += 4096) result += String.fromCodePoint(...points.slice(start, start + 4096));
  return result;
}

/** One path component as it is compared: NFD, full case folding, NFD; ASCII is lower-cased. */
export function normalizePathComponent(value: string): string {
  if (/^[\x00-\x7f]*$/.test(value)) return value.replace(/[A-Z]/g, upper => String.fromCharCode(upper.charCodeAt(0) + 32));
  const data = load();
  const decomposed = nfd(codePoints(value), data);
  const folded = decomposed.flatMap(point => data.casefold.get(point) ?? [point]);
  return fromPoints(nfd(folded, data));
}
