import fs from 'node:fs';
import path from 'node:path';
import { trySafe } from './try-safe.js';

export function dirExists(dirPath: string): boolean {
  return trySafe(
    () => fs.existsSync(dirPath) && fs.statSync(dirPath).isDirectory(),
    false
  );
}

export function fileExists(filePath: string): boolean {
  return trySafe(
    () => fs.existsSync(filePath) && fs.statSync(filePath).isFile(),
    false
  );
}

export function readFileContent(filePath: string): string | null {
  return trySafe(() => {
    if (fileExists(filePath)) {
      return fs.readFileSync(filePath, 'utf8');
    }
    return null;
  }, null);
}

export function writeFileContent(filePath: string, content: string): boolean {
  return trySafe(() => {
    const dir = path.dirname(filePath);
    if (!dirExists(dir)) {
      fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    }
    fs.writeFileSync(filePath, content, { encoding: 'utf8', mode: 0o600 });
    return true;
  }, false);
}

export function backupFile(filePath: string): string | null {
  if (!fileExists(filePath)) {
    return null;
  }
  return trySafe(() => {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupPath = `${filePath}.backup-${timestamp}`;
    fs.copyFileSync(filePath, backupPath);
    return backupPath;
  }, null);
}

export function readJsonFile<T>(filePath: string): T | null {
  const content = readFileContent(filePath);
  if (!content) return null;
  return trySafe(() => JSON.parse(content) as T, null);
}

export function writeJsonFile(filePath: string, data: unknown): boolean {
  return trySafe(() => {
    const content = JSON.stringify(data, null, 2) + '\n';
    return writeFileContent(filePath, content);
  }, false);
}

export function copyDirectory(src: string, dest: string): boolean {
  return trySafe(() => {
    if (!dirExists(src)) {
      return false;
    }
    if (!dirExists(dest)) {
      fs.mkdirSync(dest, { recursive: true });
    }
    const entries = fs.readdirSync(src, { withFileTypes: true });
    for (const entry of entries) {
      const srcPath = path.join(src, entry.name);
      const destPath = path.join(dest, entry.name);
      if (entry.isDirectory()) {
        if (!copyDirectory(srcPath, destPath)) {
          return false;
        }
      } else {
        fs.copyFileSync(srcPath, destPath);
      }
    }
    return true;
  }, false);
}

export function listSubdirectories(dirPath: string): string[] {
  return trySafe(() => {
    if (!dirExists(dirPath)) {
      return [];
    }
    return fs
      .readdirSync(dirPath, { withFileTypes: true })
      .filter(entry => {
        if (entry.isDirectory()) return true;
        if (entry.isSymbolicLink()) {
          try {
            return fs.statSync(path.join(dirPath, entry.name)).isDirectory();
          } catch {
            return false;
          }
        }
        return false;
      })
      .map(entry => entry.name);
  }, []);
}

export function removeDirectory(dirPath: string): boolean {
  return trySafe(() => {
    if (!dirExists(dirPath)) {
      return false;
    }
    fs.rmSync(dirPath, { recursive: true, force: true });
    return true;
  }, false);
}
