import { existsSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Source modules, split package chunks and standalone skill bundles locate their
// own assets. An embedding host's argv/cwd never identifies an Awareness bundle.
const here = dirname(fileURLToPath(import.meta.url));
const candidates = [
  ...(basename(here) === 'scripts' && basename(dirname(here)) === 'octocode-awareness'
    ? [resolve(here, '..', '..')] : []),
  join(here, 'skills'),
  resolve(here, '..', 'skills'),
  resolve(here, '..', '..', 'skills'),
  ...(process.env.OCTOCODE_SKILL_ROOT ? [resolve(process.env.OCTOCODE_SKILL_ROOT, '..')] : []),
];
export const BUNDLED_SKILLS_DIR = candidates.find(candidate =>
  existsSync(join(candidate, 'octocode-awareness', 'SKILL.md')),
) ?? candidates[0]!;

export function packageSkillScriptPath(...segments: string[]): string {
  return join(BUNDLED_SKILLS_DIR, 'octocode-awareness', 'scripts', ...segments);
}
