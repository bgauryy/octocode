import type {
  CloneResultData,
  CloneStructuredContent,
  DirectToolResult,
  FetchDirectoryData,
  FetchFileData,
  FetchStructuredContent,
  RemoteMaterializationKind,
} from './types.js';

export function directToolText(result: DirectToolResult): string {
  const text = (result.content ?? [])
    .filter(item => item.type === 'text' && typeof item.text === 'string')
    .map(item => item.text)
    .join('\n')
    .trim();
  if (text.length > 0) return text;
  return JSON.stringify(result.structuredContent ?? result, null, 2);
}

export function parseCloneResult(result: DirectToolResult): CloneResultData {
  const structured = result.structuredContent as
    CloneStructuredContent | undefined;
  return structured?.results?.[0]?.data ?? {};
}

export function parseFetchResult(
  result: DirectToolResult,
  kind: Extract<RemoteMaterializationKind, 'file' | 'tree'>
): FetchFileData | FetchDirectoryData {
  const structured = result.structuredContent as
    FetchStructuredContent | undefined;
  const first = structured?.results?.[0];
  const data = first?.data ?? first;
  if (kind === 'file') return data?.files?.[0] ?? {};
  return data?.directories?.[0] ?? {};
}
