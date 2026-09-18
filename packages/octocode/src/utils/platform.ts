import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import os from 'node:os';

export const isWindows = os.platform() === 'win32';
export const isMac = os.platform() === 'darwin';
export const HOME = os.homedir();
export function getAppDataPath(): string {
  return isWindows
    ? process.env.APPDATA || path.join(HOME, 'AppData', 'Roaming')
    : HOME;
}

const GIT_DIRS = new Set(['.git', '.svn', '.hg', '.bzr']);

const IDE_DIRS = new Set([
  '.vscode',
  '.idea',
  '.vs',
  '.eclipse',
  '.vscode-test',
]);

export function isGitRelated(pathToCheck: string): boolean {
  const basename = path.basename(pathToCheck);
  return GIT_DIRS.has(basename);
}

export function isIDERelated(pathToCheck: string): boolean {
  const basename = path.basename(pathToCheck);
  return IDE_DIRS.has(basename);
}

export function isInsideGitRepo(directory: string): boolean {
  try {
    let currentDir = path.resolve(directory);
    let parentDir = path.dirname(currentDir);

    while (currentDir !== parentDir) {
      const gitPath = path.join(currentDir, '.git');

      if (fs.existsSync(gitPath)) {
        return true;
      }
      currentDir = parentDir;
      parentDir = path.dirname(currentDir);
    }

    if (fs.existsSync(path.join(currentDir, '.git'))) {
      return true;
    }

    return false;
  } catch {
    return false;
  }
}

export function findGitRoot(directory: string): string | null {
  try {
    let currentDir = path.resolve(directory);
    let parentDir = path.dirname(currentDir);

    while (currentDir !== parentDir) {
      if (fs.existsSync(path.join(currentDir, '.git'))) {
        return currentDir;
      }
      currentDir = parentDir;
      parentDir = path.dirname(currentDir);
    }

    if (fs.existsSync(path.join(currentDir, '.git'))) {
      return currentDir;
    }

    return null;
  } catch {
    return null;
  }
}

export function isIDEOrGitPath(pathToCheck: string): boolean {
  return isIDERelated(pathToCheck) || isGitRelated(pathToCheck);
}

export function clearScreen(): void {
  const clearSequence = '\x1b[2J\x1b[3J\x1b[H';
  process.stdout.write(clearSequence);
}

export function openFile(filePath: string, editor?: string): boolean {
  try {
    let command: string;
    let args: string[];

    if (editor) {
      command = editor;
      args = [filePath];
    } else if (isMac) {
      command = 'open';
      args = [filePath];
    } else if (isWindows) {
      command = 'cmd';
      args = ['/c', 'start', '""', filePath];
    } else {
      command = 'xdg-open';
      args = [filePath];
    }

    const result = spawnSync(command, args, {
      stdio: 'ignore',
      shell: isWindows && !editor,
    });

    return result.status === 0;
  } catch {
    return false;
  }
}

export function openInEditor(
  filePath: string,
  ide: 'cursor' | 'vscode' | 'default'
): boolean {
  switch (ide) {
    case 'cursor':
      return openFile(filePath, 'cursor');
    case 'vscode':
      return openFile(filePath, 'code');
    case 'default':
    default:
      return openFile(filePath);
  }
}
