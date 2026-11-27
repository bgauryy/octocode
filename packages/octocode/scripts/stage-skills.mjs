import { cpSync, existsSync, lstatSync, readFileSync, rmSync } from 'node:fs';
import { basename, join, relative, resolve, sep } from 'node:path';
import { build as buildCommon } from '../../../skills/common/build.mjs';

const EXCLUDED = new Set([
  '__pycache__', 'coverage', 'dist', 'node_modules', 'out', 'target', 'tests',
  'Thumbs.db', 'npm-debug.log', 'yarn-error.log',
]);

/**
 * Skills owned by the package whose tool they guide, as repo-root-relative folders that each hold
 * `<skill>/SKILL.md`. They are self-contained (no shared bundles) and stage beside the `skills/` ones.
 */
export const PACKAGE_SKILLS = ['packages/octocode-chrome-devtools/skills'];

/** The only skill staging path used by build and prepack. */
export function stageSkills(source, target) {
  if (!existsSync(source)) throw new Error(`Skills source not found: ${source}`);
  const root = resolve(source, '..');
  buildCommon({ root });
  rmSync(target, { recursive: true, force: true });
  for (const base of [source, ...PACKAGE_SKILLS.map(dir => join(root, dir))]) {
    // A root without a package skill (a synthetic checkout in tests) stages only its own skills.
    if (base === source || existsSync(base)) copyInto(base, target);
  }
}

function copyInto(source, target) {
  const manifests = new Map();
  cpSync(source, target, {
    recursive: true,
    filter: path => {
      const name = basename(path);
      if (name.startsWith('.') || EXCLUDED.has(name) || lstatSync(path).isSymbolicLink()) return false;
      if (lstatSync(path).isDirectory() && existsSync(join(path, 'package.json')) &&
          !existsSync(join(path, 'SKILL.md')) &&
          JSON.parse(readFileSync(join(path, 'package.json'), 'utf8')).private === true) return false;
      const [skill, ...parts] = relative(source, path).split(sep);
      if (!skill || !parts.length) return true;
      if (!manifests.has(skill)) {
        const manifest = join(source, skill, 'package.json');
        const files = existsSync(manifest) ? JSON.parse(readFileSync(manifest, 'utf8')).files : undefined;
        if (files !== undefined && (!Array.isArray(files) || files.some(entry => typeof entry !== 'string' || !/^[\w.-]+(?:\/[\w.-]+)*$/.test(entry) || entry.split('/').includes('..')))) {
          throw new Error(`Skill package files must list literal relative paths: ${manifest}`);
        }
        for (const entry of files ?? []) {
          if (!existsSync(join(source, skill, entry))) throw new Error(`Missing skill bundle entry: ${skill}/${entry}. Build the skill first.`);
        }
        manifests.set(skill, files);
      }
      const files = manifests.get(skill);
      const entry = parts.join('/');
      return files === undefined || files.some(file => entry === file || entry.startsWith(`${file}/`) || file.startsWith(`${entry}/`));
    },
  });
}
