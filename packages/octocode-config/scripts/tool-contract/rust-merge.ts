import { blankLiterals, classify, itemRanges } from './rust-reach.ts';

/**
 * Merge the generated Rust types that are the same type under different names.
 *
 * typify names a type by its schema position, so a shape repeated across tools
 * (a continuation's `kind: "continue"`, a rate-limit block, a cursor input) is
 * emitted and compiled once per position. Two types are the same when their
 * definitions and impls are the same code once doc comments, layout and the
 * names of generated types are set aside, and the types they name are the
 * same in turn (the coarsest such partition, so nested shapes and recursive
 * ones merge too). Sameness is judged on the code typify emitted: serde
 * attributes, `defaults` bodies and conversions included, so a merged type
 * encodes, decodes and validates exactly as each copy did.
 *
 * Each set becomes one type and every reference is rewritten to it. A name in
 * `kept` (one native source names, or one the generated dispatch needs)
 * survives under that name: a set holding one takes it, and two such names
 * stay two types (with the structs that hold them), since merging them would
 * change a type native names. Any other set takes `sharedName`.
 *
 * Returns the merged file and the name each merged-away type now goes by.
 */
export function mergeDuplicateTypes(
  generated: string,
  kept: ReadonlySet<string>
): { rust: string; renamed: ReadonlyMap<string, string> } {
  const blanked = blankLiterals(generated);
  const items = itemRanges(blanked).map(([start, end]) => ({ start, end, item: classify(blanked.slice(start, end)) }));
  const names = items.flatMap(({ item }) => (item.kind === 'type' ? [item.name] : []));
  const isType = new Set(names);
  // An impl belongs to the type it is for (`impl Display for X`, `impl X`), or,
  // for a conversion into a std type, to the one type it converts from
  // (`impl From<X> for String`, `impl From<X> for BTreeMap<K, V>`).
  const ownerOf = (header: string): string => {
    const [trait = '', target] = header.split(/\sfor\s/);
    const named = target === undefined ? undefined : /^\s*(?:(?:::)?\s*\w+\s*::\s*)*(\w+)\s*$/.exec(target)?.[1];
    if (named !== undefined && isType.has(named)) return named;
    const from = [...new Set(trait.match(IDENTIFIER) ?? [])].filter((name) => isType.has(name));
    if (from.length === 1 && from[0] !== undefined) return from[0];
    throw new Error(`generated impl without one generated owner: ${header.trim()}`);
  };
  const owners = items.map(({ item }) =>
    item.kind === 'type' ? item.name : item.kind === 'impl' ? ownerOf(item.header) : undefined
  );
  const defaults = new Map<string, string>();

  /** The code in `[start, end)` without doc attributes or layout: `⟪Type⟫` marks a generated type, `⟦f⟧` a `defaults` function. */
  const template = (start: number, end: number): string => {
    const edits: Array<[number, number, string]> = [];
    const text = generated.slice(start, end);
    const blank = blanked.slice(start, end);
    for (const doc of blank.matchAll(DOC_ATTRIBUTE)) edits.push([doc.index, doc.index + doc[0].length, '']);
    for (const name of blank.matchAll(IDENTIFIER))
      if (isType.has(name[0])) edits.push([name.index, name.index + name[0].length, `⟪${name[0]}⟫`]);
    for (const call of text.matchAll(DEFAULT_PATH))
      if (call[1] !== undefined && defaults.has(call[1]) && blank[call.index + 1] === ' ')
        edits.push([call.index, call.index + call[0].length, `⟦${call[1]}⟧`]);
    let out = text;
    for (const [from, to, value] of edits.sort((a, b) => b[0] - a[0])) out = out.slice(0, from) + value + out.slice(to);
    return out.replace(/\s+/g, ' ').replace(/ ?([^\w\s"'⟪⟫⟦⟧]) ?/g, '$1').replace(/,([)\]}>])/g, '$1').trim();
  };

  // `defaults` functions, by name, as code: a serde `default = "defaults::f"`
  // compares by what `f` returns, not by its position-derived name.
  for (const { start, end, item } of items) {
    if (item.kind !== 'mod' || item.name !== 'defaults') continue;
    const open = blanked.indexOf('{', start) + 1;
    for (const [from, to] of itemRanges(blanked.slice(open, end - 1))) {
      const name = /\bfn\s+(\w+)/.exec(blanked.slice(open + from, open + to))?.[1];
      if (name !== undefined) defaults.set(name, template(open + from, open + to).replace(/\bfn\s+\w+/, 'fn'));
    }
  }

  const signature = new Map<string, string>(names.map((name) => [name, '']));
  items.forEach(({ start, end }, index) => {
    const owner = owners[index];
    if (owner !== undefined) signature.set(owner, `${signature.get(owner)}\n${template(start, end)}`);
  });
  for (const [name, code] of signature)
    signature.set(name, code.replace(/⟦(\w+)⟧/g, (_, fn: string) => `⟦${defaults.get(fn) ?? fn}⟧`));

  // Partition refinement: split blocks until every member names the same blocks.
  const refine = (initial: ReadonlyMap<string, string>): Map<string, number> => {
    let blocks: ReadonlyMap<string, string | number> = initial;
    for (let count = new Set(initial.values()).size; ; ) {
      const ids = new Map<string, number>();
      const next = new Map<string, number>();
      for (const name of names) {
        const code = (signature.get(name) ?? '').replace(/⟪(\w+)⟫/g, (_, used: string) => `⟪${blocks.get(used)}⟫`);
        const key = `${blocks.get(name)} ${code}`;
        if (!ids.has(key)) ids.set(key, ids.size);
        next.set(name, ids.get(key) ?? 0);
      }
      if (ids.size === count) return next;
      count = ids.size;
      blocks = next;
    }
  };
  const shapes = refine(new Map(names.map((name) => [name, ''])));
  // A shape with two kept names splits into one block per kept name, plus one
  // for the rest; refining again splits whatever holds them apart.
  const keptPerShape = new Map<number, number>();
  for (const name of names) {
    if (!kept.has(name)) continue;
    const shape = shapes.get(name) ?? 0;
    keptPerShape.set(shape, (keptPerShape.get(shape) ?? 0) + 1);
  }
  const blocks = refine(new Map(names.map((name) => {
    const shape = shapes.get(name) ?? 0;
    return [name, kept.has(name) && (keptPerShape.get(shape) ?? 0) > 1 ? `kept ${name}` : `${shape}`];
  })));

  const members = new Map<number, string[]>();
  for (const name of names) {
    const block = blocks.get(name) ?? 0;
    members.set(block, [...(members.get(block) ?? []), name]);
  }
  const ranges = new Map(items.flatMap(({ start, end, item }) => (item.kind === 'type' ? [[item.name, [start, end]] as const] : [])));
  const rename = new Map<string, string>();
  const dropped = new Set<string>();
  const taken = new Set<string>();
  const sets = [...members.values()].filter((set) => set.length > 1);
  for (const set of sets) {
    const named = set.find((name) => kept.has(name));
    if (named !== undefined) taken.add(named);
  }
  for (const set of sets) {
    const representative = set.find((name) => kept.has(name)) ?? set[0] ?? '';
    const range = ranges.get(representative);
    const sole = range === undefined ? undefined : soleMember(blanked.slice(...range));
    const name = kept.has(representative) ? representative : sharedName(set, sole, (candidate) =>
      !taken.has(candidate) && (!isType.has(candidate) || set.includes(candidate)));
    taken.add(name);
    for (const member of set) {
      if (member !== representative) dropped.add(member);
      if (member !== name) rename.set(member, name);
    }
  }
  if (rename.size === 0) return { rust: generated, renamed: rename };

  const renamed = (start: number, end: number, own?: string): string => {
    const blank = blanked.slice(start, end);
    let out = generated.slice(start, end);
    const edits: Array<[number, number, string]> = [];
    for (const name of blank.matchAll(IDENTIFIER)) {
      const to = rename.get(name[0]);
      if (to !== undefined) edits.push([name.index, name.index + name[0].length, to]);
    }
    const to = own === undefined ? undefined : rename.get(own);
    if (own !== undefined && to !== undefined)
      for (const doc of blank.matchAll(DOC_ATTRIBUTE)) {
        const literal = generated.slice(start + doc.index, start + doc.index + doc[0].length);
        edits.push([doc.index, doc.index + doc[0].length, literal.split(`\`${own}\``).join(`\`${to}\``)]);
      }
    for (const [from, until, value] of edits.sort((a, b) => b[0] - a[0])) out = out.slice(0, from) + value + out.slice(until);
    return out;
  };

  // Two variants whose types merged would give an enum two `From` impls for one
  // type; typify emits `From` only for a variant type no other variant has.
  const impls = new Map<string, number[]>();
  items.forEach(({ start, end, item }, index) => {
    const owner = owners[index];
    if (item.kind !== 'impl' || owner === undefined || dropped.has(owner)) return;
    const header = template(start, end).replace(/⟪(\w+)⟫/g, (_, used: string) => rename.get(used) ?? used);
    const key = header.slice(0, header.indexOf('{'));
    impls.set(key, [...(impls.get(key) ?? []), index]);
  });
  const ambiguous = new Set<number>();
  for (const [header, indexes] of impls) {
    if (indexes.length < 2) continue;
    if (!header.startsWith('impl::std::convert::From<')) throw new Error(`merging generated types repeats ${header}`);
    for (const index of indexes) ambiguous.add(index);
  }

  const referenced = (text: string): Set<string> =>
    new Set([...text.matchAll(/\bdefaults::(\w+)/g)].flatMap((match) => (match[1] === undefined ? [] : [match[1]])));
  const before = referenced(generated);
  const survivors: Array<{ index: number; text: string }> = [];
  items.forEach(({ start, end, item }, index) => {
    const owner = owners[index];
    if ((owner !== undefined && dropped.has(owner)) || ambiguous.has(index)) return;
    survivors.push({ index, text: item.kind === 'mod' ? '' : renamed(start, end, item.kind === 'type' ? item.name : undefined) });
  });
  const after = referenced(survivors.map(({ text }) => text).join('\n'));
  // Items are contiguous (each holds the layout before it), so the file is its
  // surviving items in order, then whatever follows the last item.
  const out: string[] = [];
  for (const { index, text } of survivors) {
    const entry = items[index];
    if (entry === undefined) continue;
    if (entry.item.kind !== 'mod') {
      out.push(text);
      continue;
    }
    // A `defaults` function only a dropped type's attributes called goes with it.
    const open = blanked.indexOf('{', entry.start) + 1;
    let from = entry.start;
    for (const [start, end] of itemRanges(blanked.slice(open, entry.end - 1))) {
      const name = /\bfn\s+(\w+)/.exec(blanked.slice(open + start, open + end))?.[1];
      if (entry.item.name === 'defaults' && name !== undefined && before.has(name) && !after.has(name)) {
        out.push(renamed(from, open + start));
        from = open + end;
      }
    }
    out.push(renamed(from, entry.end));
  }
  return { rust: out.join('') + generated.slice(items.at(-1)?.end ?? 0), renamed: rename };
}

const IDENTIFIER = /[A-Za-z_][A-Za-z0-9_]*/g;
const DOC_ATTRIBUTE = /#\[doc\s*=\s*r?#*"[^"]*"#*\]/g;
const DEFAULT_PATH = /"defaults::(\w+)"/g;

/** `GhGetFileContentQueryContent0View` → `Gh`, `Get`, `File`, `Content`, `Query`, `Content0`, `View`. */
const words = (name: string): string[] => name.match(/[A-Z][a-z]*[0-9]*|[a-z]+[0-9]*|[0-9]+/g) ?? [name];

/** `next_cursor` → `NextCursor`, `r#type` → `Type`. */
const pascalCase = (field: string): string =>
  field.replace(/^r#/, '').split('_').filter(Boolean).map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join('');

/** `blankedItem` without the attributes before it. */
function withoutAttributes(blankedItem: string): string {
  let rest = blankedItem.trimStart();
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

/** The one field of a struct, or the one unit variant of an enum, as a type-name word. */
function soleMember(blankedItem: string): string | undefined {
  const body = /^pub (struct|enum) \w+\s*\{([\s\S]*)\}\s*$/.exec(withoutAttributes(blankedItem));
  if (!body?.[2]) return undefined;
  const members: string[] = [];
  let depth = 0;
  let from = 0;
  const inner = body[2];
  for (let i = 0; i <= inner.length; i += 1) {
    const c = inner[i];
    if (c === '(' || c === '[' || c === '{' || c === '<') depth += 1;
    else if (c === ')' || c === ']' || c === '}' || c === '>') depth -= 1;
    else if ((c === ',' && depth === 0) || c === undefined) {
      const member = withoutAttributes(inner.slice(from, i)).trim();
      if (member !== '') members.push(member);
      from = i + 1;
    }
  }
  const [member] = members;
  if (members.length !== 1 || member === undefined) return undefined;
  if (body[1] === 'enum') return /^\w+$/.test(member) ? member : undefined;
  const field = /^pub (r#\w+|\w+)\s*:/.exec(member)?.[1];
  return field === undefined ? undefined : pascalCase(field);
}

/**
 * The name a set of merged types takes when native names none of them, from
 * the words its names share: `<common prefix><common suffix>`
 * (`GraphQueryDataVariant0Coverage`, `GraphQueryDataVariant1Coverage` →
 * `GraphQueryDataCoverage`) when that is two words or more; for a type with
 * one field or one unit variant, that member before the shared suffix
 * (`kind: "continue"` → `ContinueKind`, `{ cursor }` → `CursorInput`), first
 * when the suffix is one word; then the words every name holds, in order,
 * when they are more than the suffix; else its shortest name. `free` rejects
 * a name another type holds.
 */
function sharedName(set: readonly string[], sole: string | undefined, free: (name: string) => boolean): string {
  const split = set.map(words);
  const shortest = Math.min(...split.map((parts) => parts.length));
  const first = split[0] ?? [];
  const common = (at: (parts: string[], index: number) => string | undefined): number => {
    let length = 0;
    while (length < shortest && split.every((parts) => at(parts, length) === at(first, length))) length += 1;
    return length;
  };
  const prefix = common((parts, index) => parts[index]);
  const suffix = Math.min(common((parts, index) => parts[parts.length - 1 - index]), shortest - prefix);
  const role = first.slice(first.length - suffix).join('');
  const byMember = sole !== undefined && suffix > 0 ? [role.startsWith(sole) ? role : `${sole}${role}`] : [];
  const byNames = suffix > 0 && prefix + suffix >= 2 ? [`${first.slice(0, prefix).join('')}${role}`] : [];
  // The words every name holds, in order, ending in the role: what the copies
  // share when the role alone is taken (`QueryContextLines`).
  const shared: string[] = [];
  const at = split.map(() => 0);
  for (const word of first)
    if (split.every((parts, index) => parts.indexOf(word, at[index]) >= 0)) {
      split.forEach((parts, index) => { at[index] = parts.indexOf(word, at[index]) + 1; });
      shared.push(word);
    }
  const byShared = suffix > 0 && shared.length > suffix && shared.join('').endsWith(role) ? [shared.join('')] : [];
  // A one-word role (`Kind`, `Operation`) says little next to an owner prefix;
  // a longer one (`ArgumentCoverageScope`) reads best under the owner.
  const candidates = [...(suffix === 1 ? [...byMember, ...byNames] : [...byNames, ...byMember]), ...byShared];
  return candidates.find(free)
    ?? [...set].sort((left, right) => left.length - right.length || (left < right ? -1 : left > right ? 1 : 0))[0]
    ?? '';
}
