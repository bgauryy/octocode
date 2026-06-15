import { nativeBinding } from './native.js';

export async function resolveWorkspaceRootForFile(
  filePath: string,
  _workspaceRoot?: string
): Promise<string> {
  return nativeBinding.resolveWorkspaceRootForFile(filePath);
}

export async function findWorkspaceRoot(filePath: string): Promise<string> {
  return resolveWorkspaceRootForFile(filePath);
}
