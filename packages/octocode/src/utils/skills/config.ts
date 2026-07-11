import { join } from 'node:path';
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { HOME, isWindows, getAppDataPath } from '../platform.js';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import { trySafe } from '../try-safe.js';
import { z } from '@octocodeai/octocode-tools-core/zod';

const OCTOCODE_DIR = paths.home;
const CONFIG_FILE = paths.cliConfig;

const OctocodeConfigSchema = z
  .object({
    skillsDestDir: z.string().optional(),
  })
  .passthrough();

type OctocodeConfig = z.infer<typeof OctocodeConfigSchema>;

function loadConfig(): OctocodeConfig {
  return trySafe(() => {
    if (existsSync(CONFIG_FILE)) {
      const content = readFileSync(CONFIG_FILE, 'utf-8');
      const parsed = OctocodeConfigSchema.safeParse(JSON.parse(content));
      return parsed.success ? parsed.data : {};
    }
    return {};
  }, {});
}

function saveConfig(config: OctocodeConfig): void {
  trySafe(() => {
    if (!existsSync(OCTOCODE_DIR)) {
      mkdirSync(OCTOCODE_DIR, { recursive: true, mode: 0o700 });
    }
    writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2), {
      encoding: 'utf-8',
      mode: 0o600,
    });
    return true;
  }, false);
}

export function setCustomSkillsDestDir(path: string | null): void {
  const config = loadConfig();
  if (path) {
    config.skillsDestDir = path;
  } else {
    delete config.skillsDestDir;
  }
  saveConfig(config);
}

export function getCustomSkillsDestDir(): string | null {
  const config = loadConfig();
  return config.skillsDestDir || null;
}

export function getDefaultSkillsDestDir(): string {
  if (isWindows) {
    const appData = getAppDataPath();
    return join(appData, 'Claude', 'skills');
  }
  return join(HOME, '.claude', 'skills');
}
