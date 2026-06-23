import { nativeBinding } from './native.js';

export async function resolveWorkspaceRootForFile(
  filePath: string,
  workspaceRoot?: string
): Promise<string> {
  if (workspaceRoot) return workspaceRoot;
  return nativeBinding.resolveWorkspaceRootForFile(filePath);
}

export async function findWorkspaceRoot(filePath: string): Promise<string> {
  return resolveWorkspaceRootForFile(filePath);
}
