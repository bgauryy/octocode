// Turns tool metadata (descriptions, fields, command patterns) into the
// short display strings used across the list/help/catalog views: truncated
// descriptions, `[field*, field?]` summaries, enum-value previews, and
// example commands.
import {
  buildDirectToolCommandPatterns,
  buildDirectToolExampleQuery,
  getDirectToolDescription,
  getDirectToolDisplayFields,
} from '@octocodeai/octocode-tools-core/schema';
import type { loadToolContent } from '@octocodeai/octocode-tools-core/schema';
import { findToolDefinition } from './registry.js';

export const LSP_TOOL_NAME = 'lspGetSemantics';

const RAW_LOCAL_PATH_TOOL_NAMES = new Set([
  'localSearchCode',
  'localFindFiles',
  'localGetFileContent',
  'localViewStructure',
]);
const RAW_LOCAL_PATH_GUIDANCE =
  'Path note: local tools need an absolute path — "." resolves against the command cwd and can mismatch.';

const DESCRIPTION_PREFIXES = new Set([
  'github',
  'local',
  'npm',
  'package',
  'search',
  'other',
]);

export function truncateDescription(desc: string, maxLen: number): string {
  if (desc.length <= maxLen) return desc;
  const cut = desc.lastIndexOf(' ', maxLen - 1);
  return cut > maxLen * 0.6
    ? desc.slice(0, cut) + '…'
    : desc.slice(0, maxLen - 1) + '…';
}

export function formatRequiredFields(toolName: string): string {
  if (toolName === LSP_TOOL_NAME) {
    // `type` is the only always-required field. `uri` is required for every
    // type EXCEPT workspaceSymbol (which can start from workspaceRoot +
    // symbolName), so it is marked optional here to avoid a false `uri*` —
    // the per-field schema view carries the conditional requirement.
    return '[type, uri?, symbolName?, lineHint?]';
  }

  const tool = findToolDefinition(toolName);
  if (!tool) return '';
  // Top-level fields only — filter out nested dotted paths (e.g. content.patches.ranges.file)
  const fields = getDirectToolDisplayFields(tool.name).filter(
    f => !f.name.includes('.')
  );
  const required = fields.filter(f => f.required).map(f => `${f.name}*`);
  const optional = fields.filter(f => !f.required);
  if (required.length > 0) {
    const optHint = optional.slice(0, 2).map(f => `${f.name}?`);
    const parts = optHint.length > 0 ? [...required, ...optHint] : required;
    return `[${parts.join(', ')}]`;
  }
  return `[${optional
    .slice(0, 3)
    .map(f => `${f.name}?`)
    .join(', ')}]`;
}

export function extractShortDescription(fullDescription: string): string {
  return fullDescription
    .split('\n')[0]
    .trim()
    .replace(/^##\s*/, '');
}

export function formatFullDescription(fullDescription: string): string {
  const short = extractShortDescription(fullDescription);
  const rest = fullDescription.slice(short.length).trim();
  if (!rest) return '';

  return rest
    .replace(/<\/?[a-z][a-z0-9]*>/gi, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function getEnumValues(type: string): string[] {
  const match = /^enum\((.*)\)$/.exec(type);
  if (!match) return [];

  return match[1]
    .split(',')
    .map(value => value.trim())
    .filter(Boolean);
}

function wrapPipeValues(
  values: readonly string[],
  firstPrefix: string,
  nextPrefix: string,
  maxLength = 68
): string[] {
  const lines: string[] = [];
  let current = firstPrefix;

  for (const value of values) {
    const separator =
      current === firstPrefix || current === nextPrefix ? '' : '|';
    const candidate = `${current}${separator}${value}`;
    if (candidate.length > maxLength && current !== firstPrefix) {
      lines.push(current);
      current = `${nextPrefix}${value}`;
    } else {
      current = candidate;
    }
  }

  if (current !== firstPrefix && current !== nextPrefix) {
    lines.push(current);
  }

  return lines;
}

function getFieldPreviewLines(
  toolName: string,
  fieldName: string,
  label = `${fieldName}: `
): string[] {
  const field = getDirectToolDisplayFields(toolName).find(
    item => item.name === fieldName
  );
  const values = field ? getEnumValues(field.type) : [];

  if (values.length === 0) {
    return [];
  }

  return wrapPipeValues(values, label, ''.padEnd(label.length));
}

export function getToolPreviewLines(toolName: string): string[] {
  if (toolName === LSP_TOOL_NAME) {
    return getFieldPreviewLines(toolName, 'type');
  }

  if (toolName === 'ghHistoryResearch') {
    return getFieldPreviewLines(toolName, 'type');
  }

  if (toolName === 'ghSearchCode') {
    return ['keywords: array<string> (AND terms)'];
  }

  if (toolName === 'localSearchCode') {
    return ['keywords: string'];
  }

  return [];
}

export function getToolSchemaGuidance(toolName: string): string[] {
  return RAW_LOCAL_PATH_TOOL_NAMES.has(toolName)
    ? [RAW_LOCAL_PATH_GUIDANCE]
    : [];
}

export function formatConciseToolDescription(
  toolName: string,
  metadata: Awaited<ReturnType<typeof loadToolContent>> | null,
  maxLen = 88
): string {
  const raw = extractShortDescription(
    getDirectToolDescription(toolName, metadata)
  );
  const parts = raw
    .split(/\s+\|\s+/)
    .map(part => part.trim())
    .filter(Boolean);
  const concise =
    parts.find(part => !DESCRIPTION_PREFIXES.has(part.toLowerCase())) ??
    raw.replace(/^(?:github|local|npm|package|search|other)\s*\|\s*/i, '');

  return truncateDescription(concise.replace(/\s+/g, ' ').trim(), maxLen);
}

export function formatToolExampleCommand(toolName: string): string {
  const pattern = buildDirectToolCommandPatterns(toolName)[0];
  if (pattern) {
    return pattern.command;
  }

  const exampleInput = JSON.stringify(buildDirectToolExampleQuery(toolName));
  return `tools ${toolName} --queries '${exampleInput}'`;
}
