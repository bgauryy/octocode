import { spawnSync } from 'node:child_process';

/**
 * Parse `git status --porcelain=v1` / `--short` lines into paths.
 * Do NOT trim before reading the XY columns — a leading space is significant
 * (`" M file.txt"` must become `file.txt`, not `ile.txt`).
 */
export function parseGitStatusShortLines(stdout: string): string[] {
  const files: string[] = [];
  for (const rawLine of String(stdout).split('\n')) {
    if (!rawLine || rawLine.length < 4) continue;
    const xy = rawLine.slice(0, 2);
    let pathPart = rawLine.slice(3);
    // Rename/copy: keep the destination path after " -> ".
    if (xy.includes('R') || xy.includes('C')) {
      const arrow = pathPart.indexOf(' -> ');
      if (arrow >= 0) pathPart = pathPart.slice(arrow + 4);
    }
    const filePath = pathPart.trim();
    if (filePath) files.push(filePath);
  }
  return files;
}

export function gitDirtyFiles(workspacePath: string | null): string[] {
  if (!workspacePath) return [];
  try {
    const result = spawnSync('git', ['-C', workspacePath, 'status', '--porcelain=v1'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    if (result.status !== 0) return [];
    return parseGitStatusShortLines(String(result.stdout));
  } catch {
    return [];
  }
}
