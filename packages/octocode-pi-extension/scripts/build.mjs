#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const packageRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(packageRoot, '../..');
const distDir = path.join(packageRoot, 'dist');
const buildLockPath = path.join(packageRoot, '.octocode-build.lock');

const require = createRequire(import.meta.url);

// Resolve workspace/package sources via package resolution — no path hardcoding.
const CONFIG_LOADER_SRC = require.resolve('@octocodeai/config');
const AWARENESS_PACKAGE_ROOT = path.dirname(path.dirname(require.resolve('@octocodeai/octocode-awareness')));
const OCTOCODE_PACKAGE_ROOT = path.dirname(require.resolve('octocode/package.json'));

const SOURCE_PATHS = {
  // TypeScript source is compiled by tsc (see compileTsc()). Only non-code assets — each is copied flat into dist/.
  // are managed here. @octocodeai/config source injected as octocode-config.mjs into every skill that
  // has a scripts/ directory — zero npm publish dependency for standalone skills.
  configLoader: CONFIG_LOADER_SRC,
  subagents: path.join(packageRoot, 'subagents'),
  skills: path.join(packageRoot, 'skills'),
  // The system prompt is one inlined document (src/prompts/prompt.ts → dist/prompts/prompt.js);
  // there are no per-section fragment files to copy.
  promptSource: path.join(packageRoot, 'src', 'prompts', 'prompt.ts'),
  // Awareness skill source comes from the published package; runtime invocation uses npx.
  awarenessSkills: path.join(AWARENESS_PACKAGE_ROOT, 'skills'),
  octocodeSkills: path.join(OCTOCODE_PACKAGE_ROOT, 'skills'),
  // octocode CLI — bundled at build time so the pi-extension is self-contained.
  // Optional in subset checkouts: if missing, the published `octocode` runtime dep is
  // resolved at runtime by getCLIPath() instead and bundleOctocodeCLI() skips gracefully.
  octocodeCLI: path.join(repoRoot, 'packages', 'octocode', 'out'),
  // Developer docs bundled so agents can load them at runtime via the MCP localGetFileContent
  // surface — no separate checkout or docs site needed.
  docs: path.join(packageRoot, 'docs'),
};

const OUTPUT_PATHS = {
  extension: path.join(distDir, 'index.js'),
  skills: path.join(distDir, 'skills'),
  subagents: path.join(distDir, 'subagents'),
  systemPrompt: path.join(distDir, 'system', 'SYSTEM_PROMPT.md'),
  // bundled octocode CLI — agent uses: node $OCTOCODE_CLI <command>
  cli: path.join(distDir, 'cli'),
  // Developer / agent docs — readable at runtime from dist/docs/
  docs: path.join(distDir, 'docs'),
};

const SKIPPED_DIRECTORIES = new Set([
  '.git',
  '.next',
  '.turbo',
  '__pycache__',
  'coverage',
  'dist',
  'node_modules',
  'out',
  'target',
]);
const SKIPPED_FILES = new Set([
  '.DS_Store',
  'Thumbs.db',
  'npm-debug.log',
  'yarn-error.log',
]);

const EXCLUDED_BUNDLED_SKILLS = new Set([
  // The full Awareness skill ships with the (now single) @octocodeai/octocode-awareness
  // package for separate installs; the harness uses the inline <awareness> prompt, so it
  // is not bundled as a loadable skill here (preserves the prior 0-awareness-skills bundle).
  'octocode-awareness',
  // 3D mannequin/animation workflow is intentionally not part of the coding-agent bundle.
  'octocode-mannequin',
]);

function isHiddenLocalOnlyEntry(name) {
  return name.startsWith('.') && name !== '.env.example';
}

function shouldSkipEntry(entry) {
  return (
    entry.isSymbolicLink() ||
    isHiddenLocalOnlyEntry(entry.name) ||
    SKIPPED_FILES.has(entry.name) ||
    (entry.isDirectory() && SKIPPED_DIRECTORIES.has(entry.name))
  );
}

function copyFile(source, target) {
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
}

function copyDirectory(sourceDir, targetDir) {
  fs.mkdirSync(targetDir, { recursive: true });

  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    if (shouldSkipEntry(entry)) {
      continue;
    }

    const sourcePath = path.join(sourceDir, entry.name);
    const targetPath = path.join(targetDir, entry.name);
    if (entry.isDirectory()) {
      copyDirectory(sourcePath, targetPath);
    } else if (entry.isFile()) {
      copyFile(sourcePath, targetPath);
    }
  }
}

function assertRequiredSources() {
  const requiredSources = {
    promptSource: SOURCE_PATHS.promptSource,
  };

  for (const [label, sourcePath] of Object.entries(requiredSources)) {
    if (!fs.existsSync(sourcePath)) {
      throw new Error(`Missing ${label} source: ${sourcePath}`);
    }
  }
}

function assertNoHiddenLocalOnlyEntries(targetDir) {
  const violations = [];

  function walk(currentDir) {
    for (const entry of fs.readdirSync(currentDir, { withFileTypes: true })) {
      const entryPath = path.join(currentDir, entry.name);
      if (isHiddenLocalOnlyEntry(entry.name)) {
        violations.push(path.relative(targetDir, entryPath));
      } else if (entry.isDirectory()) {
        walk(entryPath);
      }
    }
  }

  walk(targetDir);

  if (violations.length > 0) {
    throw new Error(
      `Refusing to package hidden local-only entries: ${violations.join(', ')}`
    );
  }
}

/**
 * Copy octocode-config.mjs (the @octocodeai/config source) into every skill's scripts/
 * directory so skills work standalone — no npm publish dependency ever needed.
 * Skill scripts import via: import(new URL('./octocode-config.mjs', import.meta.url).href)
 */
function injectConfigIntoSkills(skillsDir) {
  if (!fs.existsSync(SOURCE_PATHS.configLoader)) {
    throw new Error(
      `Missing config loader source: ${SOURCE_PATHS.configLoader}`
    );
  }
  let injected = 0;
  for (const entry of fs.readdirSync(skillsDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const scriptsDir = path.join(skillsDir, entry.name, 'scripts');
    if (fs.existsSync(scriptsDir)) {
      copyFile(
        SOURCE_PATHS.configLoader,
        path.join(scriptsDir, 'octocode-config.mjs')
      );
      injected++;
    }
  }
  return injected;
}

function listSkillNames(skillsDir) {
  if (!fs.existsSync(skillsDir)) return [];
  return fs
    .readdirSync(skillsDir, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => entry.name)
    .filter(skillName =>
      fs.existsSync(path.join(skillsDir, skillName, 'SKILL.md'))
    )
    .sort();
}

function assertBundledSkills() {
  return listSkillNames(OUTPUT_PATHS.skills);
}

function clean() {
  fs.rmSync(distDir, { recursive: true, force: true });
}

async function acquireBuildLock() {
  const deadline = Date.now() + 120_000;
  for (;;) {
    try {
      return fs.openSync(buildLockPath, 'wx', 0o600);
    } catch (error) {
      if (error?.code !== 'EEXIST') throw error;
      try {
        if (Date.now() - fs.statSync(buildLockPath).mtimeMs > 300_000) {
          fs.rmSync(buildLockPath, { force: true });
          continue;
        }
      } catch {
        continue;
      }
      if (Date.now() >= deadline) throw new Error(`Timed out waiting for build lock: ${buildLockPath}`);
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
}

function copySkillDirectories(sourceRoot, targetRoot) {
  if (!fs.existsSync(sourceRoot)) return 0;
  let copied = 0;
  for (const entry of fs.readdirSync(sourceRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    if (EXCLUDED_BUNDLED_SKILLS.has(entry.name)) continue;
    const src = path.join(sourceRoot, entry.name);
    if (!fs.existsSync(path.join(src, 'SKILL.md'))) continue;
    fs.rmSync(path.join(targetRoot, entry.name), { recursive: true, force: true });
    copyDirectory(src, path.join(targetRoot, entry.name));
    copied++;
  }
  return copied;
}

function refreshPackageSkills(targetRoot = SOURCE_PATHS.skills) {
  fs.rmSync(targetRoot, { recursive: true, force: true });
  fs.mkdirSync(targetRoot, { recursive: true });
  // Bundle workflow skills from dependencies, excluding prompt-owned flows such as
  // Awareness. Remaining skills become discoverable on init via the
  // resources_discover hook — no on-demand install step needed for a fresh checkout.
  // (If a user also installs the same skill globally with `octocode skill --add`,
  // Pi surfaces a [Skill conflicts] notice — expected with a self-contained bundle.)
  const octocodeCopied = copySkillDirectories(SOURCE_PATHS.octocodeSkills, targetRoot);
  const awarenessCopied = copySkillDirectories(SOURCE_PATHS.awarenessSkills, targetRoot);
  assertNoHiddenLocalOnlyEntries(targetRoot);
  if (octocodeCopied + awarenessCopied === 0) {
    throw new Error(`No Awareness/Octocode skills found in ${SOURCE_PATHS.awarenessSkills} or ${SOURCE_PATHS.octocodeSkills}`);
  }
  return { octocodeCopied, awarenessCopied };
}

function syncPackageSkills(targetRoot = SOURCE_PATHS.skills) {
  assertRequiredSources();
  const { octocodeCopied, awarenessCopied } = refreshPackageSkills(targetRoot);
  const skillNames = listSkillNames(targetRoot);
  console.log(`Synced ${skillNames.length} skill(s) into ${targetRoot}`);
  if (skillNames.length > 0) console.log(`Skills: ${skillNames.join(', ')}`);
  console.log(`Sources: octocode skills/ (${octocodeCopied}), awareness skills/ (${awarenessCopied})`);
  return skillNames;
}

/**
 * Bundle the octocode CLI into dist/cli/.
 *
 * Copies the pre-built packages/octocode/out/ directory wholesale.
 * The CLI is self-contained JS (rollup bundles); native engine addons are
 * resolved from the pi-extension's own node_modules at runtime.
 *
 * Prerequisite: `yarn workspace octocode build` must run before this step.
 */
function bundleOctocodeCLI() {
  const src = SOURCE_PATHS.octocodeCLI;
  const dest = OUTPUT_PATHS.cli;

  // Optional in subset checkouts that lack the sibling `octocode` package (e.g. a
  // slimmed publish root). When absent, the published `octocode` runtime dependency
  // is resolved from node_modules by getCLIPath() at runtime, so the build can proceed.
  if (!fs.existsSync(src)) {
    console.warn(
      `octocode CLI output not found at ${src} — skipping bundle.\n` +
        `The published \`octocode\` dependency (runtime) provides the CLI instead.\n` +
        `Run \`yarn workspace octocode build\` in the full monorepo to bundle it locally.`
    );
    return null;
  }

  const entry = path.join(src, 'octocode.js');
  if (!fs.existsSync(entry)) {
    console.warn(
      `octocode.js not found in ${src} — skipping bundle. The published \`octocode\` runtime dependency provides the CLI instead.`
    );
    return null;
  }

  copyDirectory(src, dest);
  console.log(`Octocode CLI bundled: ${dest}/octocode.js`);
  return dest;
}

function compileTsc() {
  const candidates = [
    path.join(packageRoot, 'node_modules', '.bin', 'tsc'),
    path.join(packageRoot, '..', '..', 'node_modules', '.bin', 'tsc'),
  ];
  const tscBin = candidates.find(p => fs.existsSync(p));
  if (!tscBin) {
    throw new Error(
      `Could not find tsc binary. Tried:\n${candidates.join('\n')}\nRun: yarn install`
    );
  }
  execSync(`${JSON.stringify(tscBin)} -p tsconfig.build.json`, {
    stdio: 'inherit',
    cwd: packageRoot,
  });
}

async function build() {
  const buildLock = await acquireBuildLock();
  // Each build owns a private staging tree. Concurrent builds must never delete
  // another process's input while it is copying skills into its own dist tree.
  const stagedSkills = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-skills-'));
  try {
    syncPackageSkills(stagedSkills);
    // Deterministic failure point used by the cleanup regression test. It must
    // remain after staging and before any destructive output cleanup.
    if (process.env['OCTOCODE_TEST_FAIL_BUILD_AFTER_SKILL_SYNC'] === '1') {
      throw new Error('test failure after package skill sync');
    }
    clean();

  // 1. Compile TypeScript -> dist/ (generates .js + .d.ts for all src/ modules).
  compileTsc();

  // Branded pi entry shim: pi's startup summary labels an extension by the
  // shortest unique path tail after stripping index.js — a bare dist/index.js
  // entry displays as "dist". dist/octocode/index.js re-exports the real entry
  // so the [Extensions] list reads "octocode".
  fs.mkdirSync(path.join(distDir, 'octocode'), { recursive: true });
  fs.writeFileSync(
    path.join(distDir, 'octocode', 'index.js'),
    "export * from '../index.js';\nexport { default } from '../index.js';\n",
    'utf8',
  );

  // Inline the @octocodeai/config source AS dist/env.js — index.js imports './env.js', so
  // the published extension carries the loader itself (no runtime dep, nothing to publish).
  // src/env.ts stays a workspace re-export for repo-time (tests, IDE); dist is self-contained.
  copyFile(SOURCE_PATHS.configLoader, path.join(distDir, 'env.js'));
  // The system prompt is one inlined document; compileTsc() emits dist/prompts/prompt.js.
  // Import the composed prompt and write it to dist/system/ — no per-section copy needed.
  const { SYSTEM_PROMPT } = await import(
    pathToFileURL(path.join(distDir, 'prompts', 'prompt.js')).href
  );
  fs.mkdirSync(path.dirname(OUTPUT_PATHS.systemPrompt), { recursive: true });
  fs.writeFileSync(OUTPUT_PATHS.systemPrompt, SYSTEM_PROMPT, 'utf8');
  fs.rmSync(OUTPUT_PATHS.skills, { recursive: true, force: true });
  copyDirectory(stagedSkills, OUTPUT_PATHS.skills);
  // Copy subagents/ to dist/subagents/ (SYSTEM_PROMPT.md files loaded at runtime),
  // then expand the shared {{OCTOCODE_COORDINATION}} placeholder so every typed
  // subagent inherits one canonical Awareness coordination block (no drift).
  // Copy docs/ to dist/docs/ so agents and tools can read them at runtime.
  if (fs.existsSync(SOURCE_PATHS.docs)) {
    copyDirectory(SOURCE_PATHS.docs, OUTPUT_PATHS.docs);
    console.log(`Docs bundled: ${OUTPUT_PATHS.docs}`);
  }

  if (fs.existsSync(SOURCE_PATHS.subagents)) {
    copyDirectory(SOURCE_PATHS.subagents, OUTPUT_PATHS.subagents);
    const { expandSubagentPrompt, SUBAGENT_PLACEHOLDERS } = await import(
      pathToFileURL(path.join(distDir, 'prompts', 'subagent-shared.js')).href
    );
    for (const entry of fs.readdirSync(OUTPUT_PATHS.subagents, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const promptPath = path.join(OUTPUT_PATHS.subagents, entry.name, 'SYSTEM_PROMPT.md');
      if (!fs.existsSync(promptPath)) continue;
      const expanded = expandSubagentPrompt(fs.readFileSync(promptPath, 'utf8'));
      const leftover = SUBAGENT_PLACEHOLDERS.find((p) => expanded.includes(p));
      if (leftover) throw new Error(`subagent ${entry.name}: unexpanded ${leftover}`);
      fs.writeFileSync(promptPath, expanded, 'utf8');
    }
  }
  // Inject @octocodeai/config source into every skill scripts/ dir — standalone, no npm needed.
  const configInjected = injectConfigIntoSkills(OUTPUT_PATHS.skills);

  // Skills now live ONLY in dist/skills (config-injected; surfaced at runtime via
  // the resources_discover hook for both plain-pi and the octocode-agent inline
  // factory). Remove the staging <root>/skills so Pi's package scanner can't
  // surface a SECOND copy and emit a [Skill conflicts] block. (skills/** is also
  // dropped from package.json "files", so it never ships either.)
    bundleOctocodeCLI();

    assertNoHiddenLocalOnlyEntries(distDir);

    const skillNames = assertBundledSkills();
    console.log(
      `Built @octocodeai/pi-extension with ${skillNames.length} skill(s).`
    );
    if (skillNames.length > 0) console.log(`Skills: ${skillNames.join(', ')}`);
    console.log(
      `Config loader: octocode-config.mjs injected into ${configInjected} skill script dir(s)`
    );
  } finally {
    fs.rmSync(stagedSkills, { recursive: true, force: true });
    fs.closeSync(buildLock);
    fs.rmSync(buildLockPath, { force: true });
  }
}

if (process.argv.includes('--clean')) {
  clean();
} else if (process.argv.includes('--skills-only')) {
  syncPackageSkills();
} else {
  build();
}
