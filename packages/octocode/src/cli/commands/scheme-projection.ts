// The `scheme` views are core-owned projections of the public tool catalog
// (@octocodeai/config/schema). The CLI adds only its own invocation line.

import {
  projectSchemeSelection,
  schemeUsageForms,
  type SchemeJsonObject,
  type SchemeView,
} from '@octocodeai/config/schema';

/** The CLI invocation, then one gh-CLI-style form per query branch. */
export function usageLines(tool: SchemeJsonObject): string[] {
  const name = typeof tool.name === 'string' ? tool.name : 'tool';
  const forms = schemeUsageForms(tool.querySchema);
  return [
    `octocode ${name} '{"queries":[ … ]}'`,
    ...(forms.length > 0 ? forms : [`see: scheme ${name} --view query`]),
  ];
}

/** One tool's `scheme` view, narrowed by `--select FIELD=VALUE` when given. */
export function project(
  tool: SchemeJsonObject,
  view: SchemeView,
  selection?: string
): SchemeJsonObject {
  return projectSchemeSelection(tool, view, selection, {
    usage: usageLines(tool),
  });
}
