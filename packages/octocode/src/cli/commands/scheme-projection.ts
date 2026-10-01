// The `scheme` views are core-owned projections of the public tool catalog
// (@octocodeai/config/schema). The CLI adds only its own invocation line.

import {
  projectSchemeSelection,
  projectSchemeView,
  schemeUsageForms,
  type SchemeJsonObject,
  type SchemeJsonValue,
  type SchemeView,
} from '@octocodeai/config/schema';

export type JsonValue = SchemeJsonValue;
export type JsonObject = SchemeJsonObject;
export type { SchemeView };

/** The CLI invocation, then one gh-CLI-style form per query branch. */
export function usageLines(tool: JsonObject): string[] {
  const name = typeof tool.name === 'string' ? tool.name : 'tool';
  const forms = schemeUsageForms(tool.querySchema);
  return [
    `octocode ${name} '{"queries":[ … ]}'`,
    ...(forms.length > 0 ? forms : [`see: scheme ${name} --view query`]),
  ];
}

export function project(tool: JsonObject, view: SchemeView): JsonObject {
  return projectSchemeView(tool, view, { usage: usageLines(tool) });
}

export function projectSelected(
  tool: JsonObject,
  view: SchemeView,
  selection: string | undefined
): JsonObject {
  return projectSchemeSelection(tool, view, selection, {
    usage: usageLines(tool),
  });
}
