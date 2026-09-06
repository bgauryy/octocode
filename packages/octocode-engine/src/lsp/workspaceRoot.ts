import { nativeBinding } from './native.js';

export async function resolveWorkspaceRootForFile(
  filePath: string,
  workspaceRoot?: string
): Promise<string> {
  if (workspaceRoot) return workspaceRoot;
  return nativeBinding.resolveWorkspaceRootForFile(filePath);
}
