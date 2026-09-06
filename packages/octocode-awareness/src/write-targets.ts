function addPathValue(paths: string[], value: unknown): void {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
  }
}

function addApplyPatchPaths(paths: string[], command: unknown): void {
  if (typeof command !== 'string') return;
  for (const line of command.split('\n')) {
    const addUpdateDelete = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
    if (addUpdateDelete) {
      paths.push(addUpdateDelete[1]!.trim());
      continue;
    }
    const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
    if (moveTo) paths.push(moveTo[1]!.trim());
  }
}

function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

function addQueryPaths(paths: string[], value: unknown): void {
  if (!Array.isArray(value)) return;
  for (const query of value) {
    const payload = objectOrEmpty(query);
    addPathValue(paths, payload.path);
    addPathValue(paths, payload.filePath);
    addPathValue(paths, payload.file_path);
    addPathValue(paths, payload.paths);
    addPathValue(paths, payload.filePaths);
    addPathValue(paths, payload.file_paths);
  }
}

/** Extract identifiable mutation targets from host tool payloads without host-specific adapters. */
export function extractWriteTargetPaths(
  toolName: unknown,
  input: unknown = {},
  options: { assumeWrite?: boolean } = {},
): string[] {
  const normalizedToolName = String(toolName ?? '').toLowerCase();
  const isWriteTool = Boolean(options.assumeWrite) || [
    'write',
    'edit',
    'multi_edit',
    'multiedit',
    'notebookedit',
    'notebook_edit',
    'apply_patch',
    'applypatch',
  ].includes(normalizedToolName);
  const payload = objectOrEmpty(input);
  const command = typeof input === 'string'
    ? input
    : firstString(payload.command, payload.patch);

  if (!isWriteTool) {
    const patchPaths: string[] = [];
    addApplyPatchPaths(patchPaths, command);
    return [...new Set(patchPaths)];
  }

  const paths: string[] = [];
  addPathValue(paths, payload.path);
  addPathValue(paths, payload.filePath);
  addPathValue(paths, payload.file_path);
  addPathValue(paths, payload.paths);
  addPathValue(paths, payload.filePaths);
  addPathValue(paths, payload.file_paths);
  addQueryPaths(paths, payload.queries);
  addApplyPatchPaths(paths, command);

  return [...new Set(paths)];
}
