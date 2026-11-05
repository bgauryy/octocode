// One tool's `schema <tool> --view output`: every output field core describes,
// flattened from its output schema by path. A continuation or related call is
// one field: its input is the tool's own query, which the query view states.
type Json = Record<string, unknown>;

const isJson = (value: unknown): value is Json =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const refName = (ref: string): string => ref.slice(ref.lastIndexOf('/') + 1).replaceAll('~1', '/').replaceAll('~0', '~');

/** The nodes below `node`, each with the path it reads at. */
function childNodes(node: Json, path: string): Array<[unknown, string]> {
  const branches = ['anyOf', 'oneOf', 'allOf'].flatMap((key): unknown[] =>
    Array.isArray(node[key]) ? node[key] : [],
  );
  const children: Array<[unknown, string]> = branches.map(branch => [branch, path]);
  const properties = isJson(node.properties) ? node.properties : {};
  if ('kind' in properties && 'tool' in properties && 'input' in properties) return children;
  for (const [name, child] of Object.entries(properties))
    children.push([child, path ? `${path}.${name}` : name]);
  children.push([node.items, `${path}[]`], [node.additionalProperties, `${path}.*`]);
  const patterns = isJson(node.patternProperties) ? node.patternProperties : {};
  for (const child of Object.values(patterns)) children.push([child, `${path}.*`]);
  return children;
}

/** Described output fields of `schema` by path, rows' `data` read as the root. */
export function outputFields(schema: unknown, sharedDefs: unknown): Record<string, string> {
  const shared = isJson(sharedDefs) ? sharedDefs : {};
  const defs = isJson(schema) && isJson(schema.$defs) ? { ...shared, ...schema.$defs } : shared;
  const fields: Record<string, string> = {};
  const visit = (node: unknown, path: string, trail: readonly string[]): void => {
    if (!isJson(node)) return;
    if (path && typeof node.description === 'string') fields[path] ??= node.description;
    if (typeof node.$ref === 'string') {
      const name = refName(node.$ref);
      if (!trail.includes(name)) visit(defs[name], path, [...trail, name]);
    }
    for (const [child, childPath] of childNodes(node, path)) visit(child, childPath, trail);
  };
  visit(schema, '', []);
  return Object.fromEntries(
    Object.entries(fields).map(([path, text]) => [path.replace(/^results\[\]\.data\./, ''), text]),
  );
}
