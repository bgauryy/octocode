// The `schema` views are core-owned projections of the public tool catalog
// (@octocodeai/config/schema). The CLI adds only its own invocation line.

import {
  projectSchemaSelection,
  schemaUsageForms,
  type SchemaJsonObject,
  type SchemaView,
} from '@octocodeai/config/schema';

/** The CLI invocation, then one gh-CLI-style form per query branch. */
export function usageLines(tool: SchemaJsonObject): string[] {
  const name = typeof tool.name === 'string' ? tool.name : 'tool';
  const forms = schemaUsageForms(tool.querySchema);
  return [
    `octocode ${name} '{"queries":[ … ]}'`,
    ...(forms.length > 0
      ? forms
      : [`see: octocode schema ${name} --view query`]),
  ];
}

/** One tool's `schema` view, narrowed by `--select FIELD=VALUE` when given. */
export function project(
  tool: SchemaJsonObject,
  view: SchemaView,
  selection?: string
): SchemaJsonObject {
  return projectSchemaSelection(tool, view, selection, {
    usage: usageLines(tool),
  });
}
