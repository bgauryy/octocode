import { mkdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';

export function resolveWorkspaceOutputPath(output: string | null | undefined, workspacePath: string, defaultPath: string): string {
  const target = output?.trim() || defaultPath;
  return isAbsolute(target) ? resolve(target) : resolve(workspacePath, target);
}

export let atomicWriteSequence = 0;

export function atomicWriteText(path: string, content: string): void {
  mkdirSync(join(path, '..'), { recursive: true });
  const temp = `${path}.tmp-${process.pid}-${Date.now()}-${atomicWriteSequence++}`;
  try {
    writeFileSync(temp, content, { encoding: 'utf8', flag: 'wx' });
    renameSync(temp, path);
  } finally {
    rmSync(temp, { force: true });
  }
}
