/**
 * Engine-free direct-tool catalog: registry of tool definitions (name +
 * display/bulk zod schemas) plus category/sort/output-field helpers. Split out
 * of `directToolCatalog.meta.ts` (still the public barrel) — see that file's
 * header comment for the full P3 engine-free rationale.
 */
import { z } from 'zod';
import {
  STATIC_TOOL_NAMES,
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  GITHUB_SEARCH_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
} from '../toolNames.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../lsp/shared/semanticTypes.js';
import {
  DIRECT_TOOL_SPECIFICATIONS,
  type DirectToolSpecification,
} from './toolSpecifications.js';

export type DirectToolInput = Record<string, unknown> & {
  queries: unknown[];
};

export interface DirectToolDefinition {
  name: string;

  title: string;

  description: string;

  schema: z.ZodType;

  inputSchema: z.ZodType;
}

export type DirectToolCategory = 'GitHub' | 'Local Code' | 'Package' | 'Other';

export const DIRECT_TOOL_CATEGORIES: readonly DirectToolCategory[] = [
  'GitHub',
  'Local Code',
  'Package',
  'Other',
];
const DIRECT_TOOL_RELEVANCE_ORDER = new Map<string, number>(
  [
    GITHUB_SEARCH_TOOL_NAME,
    GITHUB_SEARCH_HISTORY_TOOL_NAME,
    GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
    STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    LOCAL_SEARCH_TOOL_NAME,
    LOCAL_ANALYZE_GRAPH_TOOL_NAME,
    STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    LSP_GET_SEMANTICS_TOOL_NAME,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  ].map((name, index) => [name, index])
);
export interface DirectToolDisplayField {
  name: string;
  required: boolean;
  type: string;
  /** Numeric bounds and default, e.g. "1-100, default 30" — surfaced inline so
   * agents see the full constraint without fetching the raw JSON schema. */
  constraints?: string;
  description?: string;
}

export interface DirectToolCommandPattern {
  label: string;
  query: Record<string, unknown>;
  command: string;
}

export interface DirectToolMetadata {
  tools?: Record<
    string,
    { description?: string; schema?: Record<string, string> }
  >;
}

export type DirectToolAutoFilledField = 'goal' | 'reasoning';

export interface PrepareDirectToolInputOptions {
  sourceLabel?: string;
  rejectUnknownFields?: boolean;

  onUnknownFields?: (unknownFields: string[], queryIndex: number) => void;
}

export class DirectToolInputError extends Error {
  constructor(
    message: string,
    readonly details: string[] = []
  ) {
    super(message);
    this.name = 'DirectToolInputError';
  }
}

const DIRECT_TOOL_AUTO_FILLED_FIELD_NAMES: readonly DirectToolAutoFilledField[] =
  ['goal', 'reasoning'];

export const DIRECT_TOOL_AUTO_FILLED_FIELDS: ReadonlySet<string> = new Set([
  ...DIRECT_TOOL_AUTO_FILLED_FIELD_NAMES,
]);

function toDirectToolDefinition(
  specification: DirectToolSpecification
): DirectToolDefinition {
  return {
    name: specification.name,
    title: specification.title,
    description: specification.description,
    schema: specification.schema,
    inputSchema: specification.inputSchema,
  };
}

/** Engine-free definitions, derived from the shared specification. */
export const DIRECT_TOOL_DEFINITIONS: DirectToolDefinition[] =
  DIRECT_TOOL_SPECIFICATIONS.map(specification =>
    toDirectToolDefinition(specification)
  );

/** Every public direct-tool schema. */
export const DIRECT_TOOL_DISCOVERY_DEFINITIONS: DirectToolDefinition[] = [
  ...DIRECT_TOOL_DEFINITIONS,
];

export function findDirectToolDefinition(
  name: string
): DirectToolDefinition | undefined {
  return DIRECT_TOOL_DEFINITIONS.find(tool => tool.name === name);
}

export function getDirectToolCategory(toolName: string): DirectToolCategory {
  if (toolName.startsWith('gh')) {
    return 'GitHub';
  }

  if (toolName.startsWith('local') || toolName.startsWith('lsp')) {
    return 'Local Code';
  }

  if (toolName === STATIC_TOOL_NAMES.PACKAGE_SEARCH) {
    return 'Package';
  }

  return 'Other';
}

export function sortDirectToolNames(toolNames: string[]): string[] {
  return [...toolNames].sort((left, right) => {
    const leftCategory = DIRECT_TOOL_CATEGORIES.indexOf(
      getDirectToolCategory(left)
    );
    const rightCategory = DIRECT_TOOL_CATEGORIES.indexOf(
      getDirectToolCategory(right)
    );

    if (leftCategory !== rightCategory) {
      return leftCategory - rightCategory;
    }

    const leftRank =
      DIRECT_TOOL_RELEVANCE_ORDER.get(left) ?? Number.MAX_SAFE_INTEGER;
    const rightRank =
      DIRECT_TOOL_RELEVANCE_ORDER.get(right) ?? Number.MAX_SAFE_INTEGER;

    if (leftRank !== rightRank) {
      return leftRank - rightRank;
    }

    return left.localeCompare(right);
  });
}
