import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { stageSkills } from '../../octocode/scripts/stage-skills.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const output = resolve(process.argv[2]);
const staged = join(output, 'skills');
stageSkills(join(repository, 'skills'), staged);

// Rust raw strings accept paths from every build host without JSON escape rules.
function literal(value) {
  let hashes = '#';
  while (value.includes(`"${hashes}`)) hashes += '#';
  return `r${hashes}"${value}"${hashes}`;
}
// Public skills keep `description` on one frontmatter line holding a JSON string, so the build
// decodes it here and the binary reads no frontmatter for it. `skills:check` enforces the same rule.
function description(manifest, skill) {
  const header = readFileSync(manifest, 'utf8').replace(/\r\n/g, '\n').split('\n');
  const end = header.indexOf('---', 1);
  const lines = header[0] === '---' && end > 0 ? header.slice(1, end).filter(line => line.startsWith('description:')) : [];
  let value;
  try { value = JSON.parse(lines[0]?.slice('description:'.length).trim()); } catch { /* reported below */ }
  if (lines.length !== 1 || typeof value !== 'string' || !value.trim()) {
    throw new Error(`${skill}: SKILL.md needs one \`description: "<JSON string>"\` frontmatter line`);
  }
  return value;
}
function files(directory, prefix = '') {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name < b.name ? -1 : 1).flatMap(entry => {
    const path = join(directory, entry.name);
    const relative = prefix + entry.name;
    if (entry.isDirectory()) return files(path, `${relative}/`);
    if (!entry.isFile()) throw new Error(`Unsupported skill entry: ${path}`);
    return [`EmbeddedFile { path: ${literal(relative)}, bytes: include_bytes!(${literal(path)}), executable: ${(statSync(path).mode & 0o111) !== 0} }`];
  });
}
const skills = readdirSync(staged, { withFileTypes: true }).filter(entry => entry.isDirectory()).sort((a, b) => a.name < b.name ? -1 : 1).map(entry => {
  const directory = join(staged, entry.name);
  const manifest = join(directory, 'SKILL.md');
  if (!statSync(manifest).isFile()) throw new Error(`Missing SKILL.md: ${entry.name}`);
  return `EmbeddedSkill { name: ${literal(entry.name)}, instructions: include_str!(${literal(manifest)}), description: ${literal(description(manifest, entry.name))}, files: &[${files(directory).join(',\n')}] }`;
});
if (!skills.length) throw new Error('The public skill bundle is empty');
writeFileSync(join(output, 'skills.rs'), `pub(super) static SKILLS: &[EmbeddedSkill] = &[${skills.join(',\n')}];\n`);
