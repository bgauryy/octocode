import { completeMetadata } from '@octocodeai/octocode-core';
import { getMetadataOrNull } from './state.js';
import { isLocalTool } from '../toolNames.js';

type ToolHintsType = Record<
  string,
  { hasResults: readonly string[]; empty: readonly string[] }
> & { base: { hasResults: readonly string[]; empty: readonly string[] } };

const EMPTY_HINTS = {
  hasResults: [] as readonly string[],
  empty: [] as readonly string[],
};

export const TOOL_HINTS = new Proxy({} as ToolHintsType, {
  get(_target, prop: string) {
    const metadata = getMetadataOrNull() ?? completeMetadata;
    if (prop === 'base') return metadata.baseHints;
    return metadata.tools[prop]?.hints ?? EMPTY_HINTS;
  },
  ownKeys() {
    const metadata = getMetadataOrNull() ?? completeMetadata;
    return ['base', ...Object.keys(metadata.tools)];
  },
  getOwnPropertyDescriptor(_target, prop) {
    const metadata = getMetadataOrNull() ?? completeMetadata;
    if (prop === 'base') {
      return {
        enumerable: true,
        configurable: true,
        value: metadata.baseHints,
      };
    }
    if (metadata.tools[prop as string]) {
      return {
        enumerable: true,
        configurable: true,
        value: metadata.tools[prop as string]?.hints ?? EMPTY_HINTS,
      };
    }
    return undefined;
  },
});

export function getToolHintsSync(
  toolName: string,
  resultType: 'hasResults' | 'empty'
): readonly string[] {
  const metadata = getMetadataOrNull() ?? completeMetadata;
  if (!metadata.tools[toolName]) return [];
  const rawBaseHints = metadata.baseHints?.[resultType] ?? [];
  const baseHints = isLocalTool(toolName)
    ? rawBaseHints.filter(isLocalRelevantBaseHint)
    : rawBaseHints;
  const toolHints = metadata.tools[toolName]?.hints[resultType] ?? [];
  return [...baseHints, ...toolHints];
}

function isLocalRelevantBaseHint(hint: string): boolean {
  if (hint.includes("'owner', 'repo', 'branch', 'path'")) return false;
  if (hint.includes("'mainResearchGoal'")) return false;
  return true;
}

export function getGenericErrorHintsSync(): readonly string[] {
  const metadata = getMetadataOrNull() ?? completeMetadata;
  return metadata.genericErrorHints;
}

export function getDynamicHints(
  toolName: string,
  hintType: string
): readonly string[] {
  const metadata = getMetadataOrNull() ?? completeMetadata;
  const tool = (metadata.tools as Record<string, unknown>)[toolName] as
    | { hints?: { dynamic?: Record<string, string[] | undefined> } }
    | undefined;
  return tool?.hints?.dynamic?.[hintType] ?? [];
}
