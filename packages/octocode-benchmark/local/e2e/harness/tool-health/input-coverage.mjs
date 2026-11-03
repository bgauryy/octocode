// The generated query schemas own finite values. This inventory measures explicit
// accepted inputs, not arbitrary numeric/string domains or every Cartesian product.
const keyOf = value => JSON.stringify(value);
const pointer = (root, ref) => ref?.startsWith('#/') ? ref.slice(2).split('/').reduce(
  (node, key) => node?.[key.replaceAll('~1', '/').replaceAll('~0', '~')], root) : undefined;
const finiteValues = schema => schema.enum ?? (Object.hasOwn(schema, 'const') ? [schema.const] : schema.type === 'boolean' ? [false, true] : []);
const cursorKey = (surface, tool, cursor) => JSON.stringify([surface, tool, cursor]);
const acceptedQueries = (json, queries) => (json.results ?? [])
  .filter(row => row.status !== 'error' && !row.data?.errorCode && queries[row.index])
  .map(row => queries[row.index]);

export function finiteInputDomains(root) {
  const domains = new Map();
  const visit = (schema, path, ancestors = new Set()) => {
    if (!schema || typeof schema !== 'object' || ancestors.has(schema)) return;
    const branch = new Set([...ancestors, schema]);
    const values = finiteValues(schema);
    if (values.length) {
      const id = JSON.stringify(path);
      const domain = domains.get(id) ?? { path, values: new Map() };
      for (const value of values) domain.values.set(keyOf(value), value);
      domains.set(id, domain);
    }
    visit(pointer(root, schema.$ref), path, branch);
    for (const name of ['allOf', 'anyOf', 'oneOf']) for (const item of schema[name] ?? []) visit(item, path, branch);
    for (const [name, child] of Object.entries(schema.properties ?? {})) visit(child, [...path, name], branch);
    if (schema.items) visit(schema.items, [...path, '[]'], branch);
    if (typeof schema.additionalProperties === 'object') visit(schema.additionalProperties, [...path, '*'], branch);
  };
  visit(root, []);
  return [...domains.values()].map(({ path, values }) => ({ path, values: [...values.values()] }))
    .toSorted((a, b) => JSON.stringify(a.path).localeCompare(JSON.stringify(b.path)));
}

function valuesAt(value, [part, ...rest]) {
  if (part === undefined) return [value];
  if (part === '[]') return Array.isArray(value) ? value.flatMap(item => valuesAt(item, rest)) : [];
  if (part === '*') return value && typeof value === 'object' ? Object.values(value).flatMap(item => valuesAt(item, rest)) : [];
  return value && Object.hasOwn(value, part) ? valuesAt(value[part], rest) : [];
}

export function createInputCoverage(tools, surfaces = ['cli', 'mcp']) {
  const inventory = new Map(tools.map(tool => [tool.name, finiteInputDomains(tool.querySchema).map(domain => ({
    ...domain, observed: Object.fromEntries(surfaces.map(surface => [surface, { requested: new Set(), accepted: new Set() }])),
  }))]));
  const responseQueries = new Map();
  const record = (tool, surface, queries, outcome) => {
    for (const domain of inventory.get(tool) ?? []) {
      const target = domain.observed[surface]?.[outcome];
      if (!target) continue;
      const allowed = new Set(domain.values.map(keyOf));
      for (const query of queries) for (const value of valuesAt(query, domain.path)) {
        if (allowed.has(keyOf(value))) target.add(keyOf(value));
      }
    }
  };
  return {
    observe(tool, surface, input, result) {
      const queries = input?.queries ?? responseQueries.get(cursorKey(surface, tool, input?.cursor));
      if (!Array.isArray(queries)) return;
      if (input.queries) record(tool, surface, queries, 'requested');
      const json = result.json;
      if (!json || json.errorCode || json.error) return;
      record(tool, surface, acceptedQueries(json, queries), 'accepted');
      const next = json.responsePagination?.next;
      if (next?.kind === 'continue' && next.tool === tool && typeof next.input?.cursor === 'string') {
        responseQueries.set(cursorKey(surface, tool, next.input.cursor), queries);
      }
    },
    report() {
      const toolReports = [...inventory].map(([tool, domains]) => ({ tool, domains: domains.map(domain => ({
        path: domain.path, values: domain.values,
        surfaces: Object.fromEntries(surfaces.map(surface => {
          const state = domain.observed[surface];
          return [surface, {
            requested: domain.values.filter(value => state.requested.has(keyOf(value))),
            accepted: domain.values.filter(value => state.accepted.has(keyOf(value))),
            missing: domain.values.filter(value => !state.accepted.has(keyOf(value))),
          }];
        })),
      })) }));
      return {
        scope: 'Explicit enum, constant and boolean values in query schemas, including referenced/nested fields. Accepted means a non-error result row was observed. Semantic assertions are separate checks. Omitted defaults, unrestricted inputs and cross-field Cartesian products are not counted as exercised.',
        tools: toolReports,
        summary: Object.fromEntries(surfaces.map(surface => {
          const domains = toolReports.flatMap(tool => tool.domains);
          return [surface, { total: domains.reduce((sum, domain) => sum + domain.values.length, 0),
            accepted: domains.reduce((sum, domain) => sum + domain.surfaces[surface].accepted.length, 0),
            missing: domains.reduce((sum, domain) => sum + domain.surfaces[surface].missing.length, 0) }];
        })),
      };
    },
  };
}

/** Primary public variants are required on each selected tool's two surfaces. */
export function primaryInputChecks(report, selectedTools) {
  const selected = new Set(selectedTools);
  return report.tools.filter(tool => selected.has(tool.tool)).flatMap(tool => tool.domains
    .filter(domain => domain.path.length === 1 && ['operation', 'view'].includes(domain.path[0]))
    .flatMap(domain => Object.entries(domain.surfaces).map(([surface, values]) => ({
      tool: tool.tool,
      check: `input.coverage.${domain.path[0]}.${surface}`,
      ok: values.missing.length === 0,
      detail: values.missing.length ? `No accepted explicit input for ${JSON.stringify(values.missing)}`
        : `${domain.values.length} declared values accepted`,
    }))));
}
