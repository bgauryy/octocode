// Traverse only call positions declared by the owning output schema. Evidence
// may contain arbitrary objects that resemble executable calls.
export const canon = value => JSON.stringify(sorted(value));
function sorted(value) {
  if (Array.isArray(value)) return value.map(sorted);
  return value && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).toSorted().map(key => [key, sorted(value[key])])) : value;
}
export const short = (value, length = 240) => (typeof value === 'string' ? value : JSON.stringify(value) ?? '').slice(0, length);

function members(schema, root, seen = new Set()) {
  if (!schema || typeof schema !== 'object' || seen.has(schema)) return [];
  seen.add(schema);
  const ref = schema.$ref?.startsWith('#/') ? schema.$ref.slice(2).split('/').reduce((node, part) => node?.[part.replace(/~1/g, '/').replace(/~0/g, '~')], root) : null;
  return [schema, ...members(ref, root, seen), ...['allOf', 'anyOf', 'oneOf'].flatMap(key => (schema[key] ?? []).flatMap(branch => members(branch, root, seen)))];
}
const properties = (schemas, root, key) => schemas.flatMap(schema => members(schema, root).flatMap(member => member.properties?.[key] ? [member.properties[key]] : []));
function children(schemas, root, key) {
  return schemas.flatMap(schema => members(schema, root).flatMap(member => {
    if (Object.hasOwn(member.properties ?? {}, key)) return [member.properties[key]];
    return typeof member.additionalProperties === 'object' ? [member.additionalProperties] : [];
  }));
}

/** Every declared call, preserving its complete input and output position. */
export function leadsIn(json, schema) {
  const found = [];
  const walk = (node, schemas, path, rowIndex) => {
    if (!node || typeof node !== 'object' || !schemas.length) return;
    if (['kind', 'tool', 'input'].every(key => properties(schemas, schema, key).length)) {
      if (typeof node.tool === 'string' && node.input && ['continue', 'related'].includes(node.kind)) {
        const next = path.lastIndexOf('next');
        const related = path.lastIndexOf('related');
        const key = path[0] === 'responsePagination' ? 'responsePagination.next' : path.at(-1);
        found.push({ ...node, path: path.join('.'), segments: path, key, container: related >= 0 ? 'related' : next >= 0 ? 'next' : path.at(-1), rowIndex });
      }
      return;
    }
    if (Array.isArray(node)) {
      const items = schemas.flatMap(s => members(s, schema).flatMap(m => m.items ? [m.items] : []));
      node.forEach((child, i) => walk(child, items, [...path, String(i)], path.at(-1) === 'results' ? i : rowIndex));
    } else for (const [key, child] of Object.entries(node)) walk(child, children(schemas, schema, key), [...path, key], rowIndex);
  };
  walk(json, [schema], [], null);
  return found;
}

/** Cursor bytes vary by runtime. Only their opaque presence is comparable. */
export const graphInput = input => Object.hasOwn(input, 'cursor') ? { cursor: '<issued>' } : input;

/** Map only declared output fields; arbitrary evidence values remain untouched. */
export function mapDeclared(json, schema, transform) {
  const walk = (node, schemas, path) => {
    if (!schemas.length) return node;
    const transformed = transform(node, path);
    if (transformed !== node) return transformed;
    if (Array.isArray(node)) {
      const items = schemas.flatMap(s => members(s, schema).flatMap(m => m.items ? [m.items] : []));
      return node.map((child, i) => walk(child, items, [...path, String(i)])).filter(child => child !== undefined);
    }
    if (!node || typeof node !== 'object') return node;
    return Object.fromEntries(Object.entries(node).map(([key, child]) => [key, walk(child, children(schemas, schema, key), [...path, key])]).filter(([, child]) => child !== undefined));
  };
  return walk(json, [schema], []);
}
