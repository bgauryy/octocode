// One tool's `schema <tool> --view output`: every output field core describes,
// flattened from its output schema by path. A continuation or related call is
// one field: its input is the tool's own query, which the query view states.
type Json = Record<string, unknown>;

const isJson = (value: unknown): value is Json =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const refName = (ref: string): string =>
  ref
    .slice(ref.lastIndexOf('/') + 1)
    .replaceAll('~1', '/')
    .replaceAll('~0', '~');

/** `text`'s sentences, each with its closing stop. */
const sentences = (text: string): string[] =>
  text.split(/(?<=[.!?]) +(?=[A-Za-z(`"'])/u).filter(Boolean);

/** The nodes below `node`, each with the path it reads at. */
function childNodes(node: Json, path: string): Array<[unknown, string]> {
  const branches = ['anyOf', 'oneOf', 'allOf'].flatMap((key): unknown[] =>
    Array.isArray(node[key]) ? node[key] : []
  );
  const children: Array<[unknown, string]> = branches.map(branch => [
    branch,
    path,
  ]);
  const properties = isJson(node.properties) ? node.properties : {};
  if ('kind' in properties && 'tool' in properties && 'input' in properties)
    return children;
  for (const [name, child] of Object.entries(properties))
    children.push([child, path ? `${path}.${name}` : name]);
  children.push(
    [node.items, `${path}[]`],
    [node.additionalProperties, `${path}.*`]
  );
  const patterns = isJson(node.patternProperties) ? node.patternProperties : {};
  for (const child of Object.values(patterns))
    children.push([child, `${path}.*`]);
  return children;
}

/**
 * Described output fields of `schema` by path, rows' `data` read as the root.
 * A field's own description stands for it: the schema it refers to, or the
 * branches it is one of at the same path, add none. Variants that share a
 * path (the row forms of different operations) each state their meaning, in
 * schema order, so no operation's field reads as another's; a sentence they
 * share is stated once.
 */
export function outputFields(
  schema: unknown,
  sharedDefs: unknown
): Record<string, string> {
  const shared = isJson(sharedDefs) ? sharedDefs : {};
  const defs =
    isJson(schema) && isJson(schema.$defs)
      ? { ...shared, ...schema.$defs }
      : shared;
  const fields: Record<string, string[]> = {};
  const visit = (
    node: unknown,
    path: string,
    trail: readonly string[],
    described: boolean
  ): void => {
    if (!isJson(node)) return;
    const own = path !== '' && typeof node.description === 'string';
    if (own && !described) {
      // A sentence an earlier variant at this path states is stated once.
      const texts = (fields[path] ??= []);
      for (const sentence of sentences(node.description as string))
        if (!texts.includes(sentence)) texts.push(sentence);
    }
    const stated = described || own;
    if (typeof node.$ref === 'string') {
      const name = refName(node.$ref);
      if (!trail.includes(name))
        visit(defs[name], path, [...trail, name], stated);
    }
    for (const [child, childPath] of childNodes(node, path))
      visit(child, childPath, trail, stated && childPath === path);
  };
  visit(schema, '', [], false);
  return Object.fromEntries(
    Object.entries(fields).map(([path, texts]) => [
      path.replace(/^results\[\]\.data\./, ''),
      texts.join(' '),
    ])
  );
}
