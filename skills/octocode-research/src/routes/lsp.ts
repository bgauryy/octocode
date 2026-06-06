import { Router } from 'express';
import {
  lspGotoDefinition,
  lspFindReferences,
  lspCallHierarchy,
} from '../index.js';
import {
  lspDefinitionSchema,
  lspReferencesSchema,
  lspCallsSchema,
} from '../validation/index.js';
import { ResearchResponse } from '../utils/responseBuilder.js';
import { withLspResilience } from '../utils/resilience.js';
import { createRouteHandler } from '../utils/routeFactory.js';
import { safeString, safeArray } from '../utils/responseFactory.js';
import { isObject, hasProperty, hasNumberProperty, hasStringProperty } from '../types/guards.js';

export const lspRoutes = Router();

lspRoutes.get(
  '/lspGotoDefinition',
  createRouteHandler({
    schema: lspDefinitionSchema,
    toolFn: lspGotoDefinition,
    toolName: 'lspGotoDefinition',
    resilience: withLspResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const locations = extractLocations(data, 'definition');

      return ResearchResponse.lspResult({
        symbol: queries[0]?.symbolName || 'unknown',
        locations,
        type: 'definition',
        mcpHints: hints,
        research,
      });
    },
  })
);

lspRoutes.get(
  '/lspFindReferences',
  createRouteHandler({
    schema: lspReferencesSchema,
    toolFn: lspFindReferences,
    toolName: 'lspFindReferences',
    resilience: withLspResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const locations = extractLocations(data, 'references');

      return ResearchResponse.lspResult({
        symbol: queries[0]?.symbolName || 'unknown',
        locations,
        type: 'references',
        mcpHints: hints,
        research,
      });
    },
  })
);

lspRoutes.get(
  '/lspCallHierarchy',
  createRouteHandler({
    schema: lspCallsSchema,
    toolFn: lspCallHierarchy,
    toolName: 'lspCallHierarchy',
    resilience: withLspResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const locations = extractCallHierarchyLocations(data);
      const direction = queries[0]?.direction || 'incoming';

      return ResearchResponse.lspResult({
        symbol: queries[0]?.symbolName || 'unknown',
        locations,
        type: direction as 'incoming' | 'outgoing',
        mcpHints: hints,
        research,
      });
    },
  })
);

type LspLocation = { uri: string; line: number; preview?: string };

function extractStartLine(obj: Record<string, unknown>): number {
  const range = isObject(obj.range) ? obj.range : {};
  const start = isObject(range.start) ? range.start : {};
  return (hasNumberProperty(start, 'line') ? start.line : 0) + 1;
}

function extractDefinitionLocation(data: Record<string, unknown>): LspLocation[] {
  if (!hasProperty(data, 'definition') || !isObject(data.definition)) return [];
  const def = data.definition as Record<string, unknown>;
  if (typeof def.uri !== 'string') return [];

  return [{
    uri: def.uri,
    line: extractStartLine(def),
    preview: typeof def.preview === 'string' ? def.preview : undefined,
  }];
}

function extractReferenceLocations(data: Record<string, unknown>): LspLocation[] {
  if (!hasProperty(data, 'references')) return [];
  return safeArray<Record<string, unknown>>(data, 'references').map((ref) => ({
    uri: safeString(ref, 'uri'),
    line: extractStartLine(ref),
    preview: hasStringProperty(ref, 'preview') ? ref.preview : undefined,
  }));
}

function extractGenericLocations(data: Record<string, unknown>): LspLocation[] {
  if (!hasProperty(data, 'locations')) return [];
  return safeArray<Record<string, unknown>>(data, 'locations').map((loc) => ({
    uri: safeString(loc, 'uri'),
    line: extractStartLine(loc),
    preview: hasStringProperty(loc, 'content') ? loc.content : undefined,
  }));
}

function extractLocations(
  data: Record<string, unknown>,
  type: 'definition' | 'references'
): LspLocation[] {
  if (type === 'definition') {
    const defs = extractDefinitionLocation(data);
    if (defs.length > 0) return defs;
  }

  if (type === 'references') {
    const refs = extractReferenceLocations(data);
    if (refs.length > 0) return refs;
  }

  return extractGenericLocations(data);
}

function getCallsArray(data: Record<string, unknown>): Record<string, unknown>[] {
  for (const key of ['calls', 'incomingCalls', 'outgoingCalls']) {
    if (hasProperty(data, key) && Array.isArray(data[key])) {
      return data[key] as Record<string, unknown>[];
    }
  }
  return [];
}

function extractCallHierarchyLocations(
  data: Record<string, unknown>
): LspLocation[] {
  return getCallsArray(data).map((call) => {
    const item = isObject(call.from) ? call.from : isObject(call.to) ? call.to : call;
    const itemObj = isObject(item) ? item as Record<string, unknown> : {};

    const lineFromRange = extractStartLine(itemObj) - 1;
    const lineFromItem = hasNumberProperty(itemObj, 'line') ? itemObj.line : 0;

    return {
      uri: safeString(itemObj, 'uri'),
      line: (lineFromRange || lineFromItem) + 1,
      preview: hasStringProperty(itemObj, 'name') ? itemObj.name : undefined,
    };
  });
}
