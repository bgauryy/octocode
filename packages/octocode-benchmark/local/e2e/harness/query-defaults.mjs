import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

/** The suites' JSON Schema 2020-12 validator; `validateFormats: false` ignores `format`. */
export function createAjv({ validateFormats = true } = {}) {
  const ajv = new Ajv2020({ strict: false, validateFormats });
  addFormats(ajv);
  return ajv;
}

export function createQueryDefaults(tools, ajv = createAjv()) {
  const schemas = new Map(tools.map(tool => [tool.name, { root: tool.querySchema ?? tool.inputSchema, isRow: tool.querySchema !== undefined }]));
  const checks = new WeakMap();
  return (tool, query) => {
    const entry = schemas.get(tool);
    if (!entry?.root) return {};
    const { root, isRow } = entry;
    const resolve = schema => {
      const seen = new Set();
      while (schema?.$ref?.startsWith('#/') && !seen.has(schema)) {
        seen.add(schema);
        schema = schema.$ref.slice(2).split('/').reduce((node, key) => node?.[key.replace(/~1/g, '/').replace(/~0/g, '~')], root);
      }
      return schema;
    };
    const accepts = schema => {
      if (!schema || typeof schema !== 'object') return schema === true;
      if (!checks.has(schema)) checks.set(schema, ajv.compile({ $defs: root.$defs, ...schema }));
      return checks.get(schema)(query);
    };
    const fieldDefault = schema => {
      schema = resolve(schema);
      return schema && Object.hasOwn(schema, 'default') ? { value: schema.default } : null;
    };
    const collect = schema => {
      schema = resolve(schema);
      if (!schema || typeof schema !== 'object') return {};
      const defaults = {};
      for (const [field, property] of Object.entries(schema.properties ?? {})) {
        const fact = fieldDefault(property);
        if (fact) defaults[field] = fact.value;
      }
      for (const branch of schema.allOf ?? []) Object.assign(defaults, collect(branch));
      for (const union of [schema.anyOf, schema.oneOf].filter(Boolean)) {
        const matching = union.map(resolve).filter(accepts).map(collect);
        for (const [field, value] of Object.entries(matching[0] ?? {})) {
          if (matching.every(branch => Object.hasOwn(branch, field) && JSON.stringify(branch[field]) === JSON.stringify(value))) defaults[field] = value;
        }
      }
      return defaults;
    };
    if (isRow) return collect(root);
    const members = (schema, key, seen = new Set()) => {
      schema = resolve(schema);
      if (!schema || typeof schema !== 'object' || seen.has(schema)) return [];
      seen.add(schema);
      return [
        ...(Object.hasOwn(schema, key) ? [schema[key]] : []),
        ...['allOf', 'anyOf', 'oneOf'].flatMap(union =>
          (schema[union] ?? []).flatMap(branch => members(branch, key, seen))),
      ];
    };
    const rows = members(root, 'properties').flatMap(properties => members(properties.queries, 'items'));
    return collect({ anyOf: rows });
  };
}
