import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Keep only the generated Rust types native code reaches.
 *
 * typify emits a type for every schema in the bundle, most of which native
 * never names (continuation and clasify branches it handles as JSON). Every
 * kept type is compiled with its serde derives on each runtime edit, so the
 * file carries only:
 * - the roots: generated type names that appear anywhere in native source;
 * - every type a kept type's definition or impls name, transitively;
 * - the `error` module and each `defaults` function whose types are kept.
 *
 * A native edit that names a type the file lacks fails to compile until the
 * contract is regenerated; `generate:tool-contract --check` reports a file
 * whose roots changed.
 */

/** `text` with comments and string or char literal contents blanked to spaces, offsets kept. */
export function blankLiterals(text: string): string {
  const parts: string[] = [];
  let kept = 0;
  const blank = (from: number, to: number): void => {
    const end = Math.min(to, text.length);
    if (from >= end) return;
    parts.push(
      text.slice(kept, from),
      text.slice(from, end).replace(/[^\n]/g, ' ')
    );
    kept = end;
  };
  const ident = /[A-Za-z0-9_]/;
  const opener = /[/r"']/g;
  const rawOpener = /r(#*)"/y;
  let i = 0;
  while (i < text.length) {
    // Only these four characters can open a comment or a literal; the source between them stays as it is.
    opener.lastIndex = i;
    const found = opener.exec(text);
    if (!found) break;
    i = found.index;
    const c = found[0];
    rawOpener.lastIndex = i;
    const raw =
      c === 'r' && !ident.test(text[i - 1] ?? '') ? rawOpener.exec(text) : null;
    if (c === '/' && text[i + 1] === '/') {
      const end = text.indexOf('\n', i);
      const stop = end === -1 ? text.length : end;
      blank(i, stop);
      i = stop;
    } else if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      const stop = end === -1 ? text.length : end + 2;
      blank(i, stop);
      i = stop;
    } else if (raw) {
      const hashes = raw[1] ?? '';
      const open = i + 1 + hashes.length + 1;
      const end = text.indexOf(`"${hashes}`, open);
      const stop = end === -1 ? text.length : end;
      blank(open, stop);
      i = stop + 1 + hashes.length;
    } else if (c === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1;
      blank(i + 1, j);
      i = j + 1;
    } else if (c === "'" && (text[i + 1] === '\\' || text[i + 2] === "'")) {
      // A char literal; a lifetime (`'de`, `'static`) has no closing quote.
      let j = i + 1;
      while (j < text.length && text[j] !== "'") j += text[j] === '\\' ? 2 : 1;
      blank(i + 1, j);
      i = j + 1;
    } else {
      i += 1;
    }
  }
  parts.push(text.slice(kept));
  return parts.join('');
}

/** Top-level items of blanked Rust source as `[start, end)` ranges; each starts after the previous one. */
export function itemRanges(blanked: string): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  let start = 0;
  let depth = 0;
  for (let i = 0; i < blanked.length; i += 1) {
    const c = blanked[i];
    if (c === '{' || c === '(' || c === '[') depth += 1;
    else if (c === '}' || c === ')' || c === ']') {
      depth -= 1;
      if (depth === 0 && c === '}') {
        ranges.push([start, i + 1]);
        start = i + 1;
      }
    } else if (c === ';' && depth === 0) {
      ranges.push([start, i + 1]);
      start = i + 1;
    }
  }
  if (blanked.slice(start).trim() !== '')
    throw new Error('generated Rust ends inside an item');
  return ranges;
}

/** The item without its leading attributes. */
function withoutAttributes(item: string): string {
  let rest = item.trimStart();
  while (rest.startsWith('#[')) {
    let depth = 0;
    let end = 0;
    for (; end < rest.length; end += 1) {
      if (rest[end] === '[') depth += 1;
      else if (rest[end] === ']' && --depth === 0) break;
    }
    rest = rest.slice(end + 1).trimStart();
  }
  return rest;
}

type Item =
  | { kind: 'type'; name: string }
  | { kind: 'impl'; header: string }
  | { kind: 'mod'; name: string };

/** What a top-level generated item declares. Unknown item kinds fail loudly. */
export function classify(blankedItem: string): Item {
  const body = withoutAttributes(blankedItem);
  const declared = /^pub (?:struct|enum|type) (\w+)/.exec(body);
  if (declared?.[1]) return { kind: 'type', name: declared[1] };
  const module = /^pub mod (\w+)/.exec(body);
  if (module?.[1]) return { kind: 'mod', name: module[1] };
  if (/^impl\b/.test(body))
    return { kind: 'impl', header: body.slice(0, body.indexOf('{')) };
  throw new Error(`unexpected generated Rust item: ${body.slice(0, 80)}`);
}

const identifiers = (text: string): Set<string> =>
  new Set(text.match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? []);

/** Each Rust source under `cratesDir` (`crates/<crate>/**\/*.rs`), with literals and comments blanked. */
function* rustSources(
  cratesDir: string
): Generator<{ path: string; text: string }> {
  for (const entry of readdirSync(cratesDir, { withFileTypes: true })) {
    const path = join(cratesDir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'target' && entry.name !== 'node_modules')
        yield* rustSources(path);
    } else if (entry.name.endsWith('.rs')) {
      yield { path, text: blankLiterals(readFileSync(path, 'utf8')) };
    }
  }
}

/** Every identifier in the native crates' Rust sources (`crates/<crate>/**\/*.rs`). */
export function nativeIdentifiers(cratesDir: string): Set<string> {
  const names = new Set<string>();
  for (const { text } of rustSources(cratesDir))
    for (const name of identifiers(text)) names.add(name);
  return names;
}

/**
 * The generated type names native source reaches through the `krate` path
 * (`krate::Name`, `krate::{A, B::C}`): the first segment of each path.
 */
export function namesThrough(cratesDir: string, krate: string): Set<string> {
  const names = new Set<string>();
  const prefix = new RegExp(`\\b${krate}\\s*::\\s*`, 'g');
  const glob = (path: string): Error =>
    new Error(`${path}: name the types of ${krate} instead of importing *`);
  for (const { path, text } of rustSources(cratesDir)) {
    for (const match of text.matchAll(prefix)) {
      const rest = text.slice(match.index + match[0].length);
      if (rest.startsWith('*')) throw glob(path);
      if (!rest.startsWith('{')) {
        const name = /^\w+/.exec(rest)?.[0];
        if (name !== undefined) names.add(name);
        continue;
      }
      // A use tree: the first segment of each top-level path in the braces.
      let depth = 0;
      let segment = true;
      for (let i = 0; i < rest.length; i += 1) {
        const c = rest[i] ?? '';
        if (c === '{') depth += 1;
        else if (c === '}' && --depth === 0) break;
        else if (c === ',' && depth === 1) segment = true;
        else if (segment && depth === 1 && c === '*') throw glob(path);
        else if (segment && depth === 1 && /\w/.test(c)) {
          const name = /^\w+/.exec(rest.slice(i))?.[0] ?? '';
          names.add(name);
          i += name.length - 1;
          segment = false;
        }
      }
    }
  }
  return names;
}

/** `generated` (typify output, inner attributes removed) keeping only what `roots` reach. */
export function keepReachable(generated: string, roots: Set<string>): string {
  const blanked = blankLiterals(generated);
  const items = itemRanges(blanked).map(([start, end]) => ({
    start,
    end,
    item: classify(blanked.slice(start, end)),
    names: identifiers(blanked.slice(start, end)),
  }));
  const types = new Set(
    items.flatMap(({ item }) => (item.kind === 'type' ? [item.name] : []))
  );
  const generatedIn = (names: Iterable<string>): string[] =>
    [...names].filter(name => types.has(name));
  // An impl belongs to the generated types its header names (`impl X`,
  // `impl From<X> for Y`, `impl From<X> for String`): it is kept when all of
  // them are, and then keeps what its body names.
  const owners = items.map(({ item }) =>
    item.kind === 'type'
      ? [item.name]
      : item.kind === 'impl'
        ? generatedIn(identifiers(item.header))
        : []
  );
  const definitions = new Map<string, string[]>();
  items.forEach(({ item, names }) => {
    if (item.kind === 'type') definitions.set(item.name, generatedIn(names));
  });
  const kept = new Set<string>();
  const pending = generatedIn(roots);
  const applied = new Set<number>();
  while (pending.length > 0) {
    for (let name = pending.pop(); name !== undefined; name = pending.pop()) {
      if (kept.has(name)) continue;
      kept.add(name);
      for (const used of definitions.get(name) ?? [])
        if (!kept.has(used)) pending.push(used);
    }
    items.forEach(({ item, names }, index) => {
      if (item.kind !== 'impl' || applied.has(index)) return;
      if (!(owners[index] ?? []).every(owner => kept.has(owner))) return;
      applied.add(index);
      pending.push(...generatedIn(names).filter(name => !kept.has(name)));
    });
  }
  const keeps = (names: Iterable<string>): boolean =>
    generatedIn(names).every(name => kept.has(name));
  const dropped: Array<[number, number]> = [];
  items.forEach(({ start, end, item }, index) => {
    if (item.kind !== 'mod') {
      if (!keeps(owners[index] ?? [])) dropped.push([start, end]);
      return;
    }
    // Module members (the `defaults` functions) stay when every generated
    // type they name is kept.
    const open = blanked.indexOf('{', start) + 1;
    const inner = blanked.slice(open, end - 1);
    for (const [from, to] of itemRanges(inner)) {
      if (!keeps(identifiers(inner.slice(from, to))))
        dropped.push([open + from, open + to]);
    }
  });
  let out = '';
  let at = 0;
  for (const [start, end] of dropped.sort((a, b) => a[0] - b[0])) {
    out += generated.slice(at, start);
    at = end;
  }
  return out + generated.slice(at);
}

/**
 * `generated` without what `other` (a `keepReachable` slice of it) declares:
 * its types, the impls only they own, the `defaults` functions only they use,
 * and the `error` module, which the crate takes from `other`'s crate.
 */
export function withoutTypesOf(generated: string, other: string): string {
  const declared = (text: string): Set<string> => {
    const blank = blankLiterals(text);
    return new Set(
      itemRanges(blank).flatMap(([start, end]) => {
        const item = classify(blank.slice(start, end));
        return item.kind === 'type' ? [item.name] : [];
      })
    );
  };
  const moved = declared(other);
  const blanked = blankLiterals(generated);
  const types = declared(generated);
  const movedAll = (names: Iterable<string>): boolean => {
    const named = [...names].filter(name => types.has(name));
    return named.length > 0 && named.every(name => moved.has(name));
  };
  const dropped: Array<[number, number]> = [];
  for (const [start, end] of itemRanges(blanked)) {
    const item = classify(blanked.slice(start, end));
    if (item.kind === 'type') {
      if (moved.has(item.name)) dropped.push([start, end]);
    } else if (item.kind === 'impl') {
      if (movedAll(identifiers(item.header))) dropped.push([start, end]);
    } else if (item.name === 'error') {
      dropped.push([start, end]);
    } else {
      const open = blanked.indexOf('{', start) + 1;
      const inner = blanked.slice(open, end - 1);
      for (const [from, to] of itemRanges(inner))
        if (movedAll(identifiers(inner.slice(from, to))))
          dropped.push([open + from, open + to]);
    }
  }
  let out = '';
  let at = 0;
  for (const [start, end] of dropped.sort((a, b) => a[0] - b[0])) {
    out += generated.slice(at, start);
    at = end;
  }
  return out + generated.slice(at);
}
