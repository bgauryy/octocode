import os from 'node:os';
import path from 'node:path';

export const isWindows: boolean = os.platform() === 'win32';

export const isMac: boolean = os.platform() === 'darwin';

export const isLinux: boolean = os.platform() === 'linux';

export const HOME: string = os.homedir();

export function getAppDataPath(): string {
  if (isWindows) {
    return process.env.APPDATA || path.join(HOME, 'AppData', 'Roaming');
  }
  return HOME;
}

export function getLocalAppDataPath(): string {
  if (isWindows) {
    return process.env.LOCALAPPDATA || path.join(HOME, 'AppData', 'Local');
  }
  return HOME;
}

export function getPlatformName(): string {
  if (isMac) return 'macOS';
  if (isWindows) return 'Windows';
  if (isLinux) return 'Linux';
  return os.platform();
}

export function getArchitecture(): string {
  return os.arch();
}
