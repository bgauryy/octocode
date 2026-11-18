import { blankLiterals, classify, itemRanges } from './rust-reach.ts';

/**
 * Serde derives by the role a type has in the tool schemas.
 *
 * typify derives `Deserialize` and `Serialize` on every type, and the
 * `Deserialize` derives are most of the generated crate's macro expansion and
 * type checking. Native decodes what a call carries: a tool's query, its bulk
 * input, and every type they reach. It also decodes some types it names
 * itself: a page or plan it saved, or a row it builds as JSON and checks
 * against its generated type. Every other output type is built in Rust and
 * only encoded, so it keeps `Serialize` alone. A type no tool schema reaches
 * (a schema core registers on its own, such as the analysis capabilities) has
 * no role to narrow by and keeps both.
 */
export type Roles = { readonly inputs: Iterable<string>; readonly outputs: Iterable<string> };

const identifiers = (text: string): Set<string> => new Set(text.match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? []);

/**
 * The types of `generated` (typify output) that must decode: those an input
 * root reaches through the type definitions (fields, variants, aliases), those
 * no input or output root reaches, those `named` (native source names them),
 * and every type these reach in turn.
 */
export function decodedTypes(generated: string, roles: Roles, named: Iterable<string> = []): Set<string> {
  const blanked = blankLiterals(generated);
  const definitions = new Map<string, string[]>();
  const items = itemRanges(blanked).map(([start, end]) => ({ item: classify(blanked.slice(start, end)), text: blanked.slice(start, end) }));
  const types = new Set(items.flatMap(({ item }) => (item.kind === 'type' ? [item.name] : [])));
  for (const { item, text } of items)
    if (item.kind === 'type') definitions.set(item.name, [...identifiers(text)].filter((name) => types.has(name)));
  const reach = (roots: Iterable<string>): Set<string> => {
    const seen = new Set<string>();
    const pending = [...roots].filter((name) => types.has(name));
    for (let name = pending.pop(); name !== undefined; name = pending.pop()) {
      if (seen.has(name)) continue;
      seen.add(name);
      pending.push(...(definitions.get(name) ?? []));
    }
    return seen;
  };
  const inputs = reach(roles.inputs);
  const outputs = reach(roles.outputs);
  // A decoded type's fields decode too, whatever else uses them.
  return reach([...inputs, ...named, ...[...types].filter((name) => !outputs.has(name))]);
}

const DESERIALIZE_DERIVE = /(#\[derive\(\s*)::\s*serde\s*::\s*Deserialize\s*,\s*/;

/**
 * `generated` with `Deserialize` only on the `decoded` types: every other
 * type loses the derive and typify's hand-written `Deserialize` impl.
 */
export function encodeOnly(generated: string, decoded: ReadonlySet<string>): string {
  const blanked = blankLiterals(generated);
  const out: string[] = [];
  let at = 0;
  for (const [start, end] of itemRanges(blanked)) {
    const item = classify(blanked.slice(start, end));
    const text = generated.slice(start, end);
    const target = item.kind === 'impl' ? deserializeTarget(item.header) : undefined;
    if (item.kind === 'type' && !decoded.has(item.name)) out.push(text.replace(DESERIALIZE_DERIVE, '$1'));
    else if (target === undefined || decoded.has(target)) out.push(text);
    at = end;
  }
  return out.join('') + generated.slice(at);
}

/** `X` for typify's `impl<'de> ::serde::Deserialize<'de> for X`. */
function deserializeTarget(header: string): string | undefined {
  return /^impl\s*<\s*'de\s*>\s*::\s*serde\s*::\s*Deserialize\s*<\s*'de\s*>\s+for\s+(\w+)\s*$/.exec(header.trim())?.[1];
}
