import { dirExists } from '../fs.js';
import { mkdirSync, rmSync } from 'node:fs';

export function prepareSkillDestination(skillDestDir: string): void {
  if (dirExists(skillDestDir)) {
    rmSync(skillDestDir, { recursive: true, force: true });
  }

  if (!dirExists(skillDestDir)) {
    mkdirSync(skillDestDir, { recursive: true, mode: 0o700 });
  }
}
