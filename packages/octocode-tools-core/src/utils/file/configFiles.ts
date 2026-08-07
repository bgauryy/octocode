/**
 * Common configuration / manifest file detection + coarse file-type
 * classification.
 *
 * `isConfigFile` combines three matching strategies:
 *  1. Exact basename match (e.g. `package.json`, `Cargo.toml`).
 *  2. Basename regex match for variadic names (e.g. `.eslintrc.*`, `*.csproj`).
 *  3. Config-ish extension match for structured formats (e.g. `.toml`, `.ini`).
 *
 * `classifyFileType` layers lock/doc/code detection on top so fetch tools can
 * tell an agent whether a path is a `code`, `config`, `lock`, `doc`, or
 * `other` file — a cheap signal that changes how the returned bytes should be
 * read.
 *
 * Config vs lock — rule of thumb: a CONFIG/manifest file declares *how* a
 * package/service/code is meant to work and *which* dependencies it wants
 * (`package.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `tsconfig.json`).
 * A LOCK file is a generated snapshot pinning the *resolved* dependency tree
 * (`yarn.lock`, `package-lock.json`, `Cargo.lock`, `go.sum`) — not authored,
 * rarely worth reading in full, and never the source of truth for intent.
 *
 * Scope is deliberately limited to config/manifest/lock files of the top
 * popular languages and platforms — not every conceivable dotfile.
 */

/** Exact config file basenames, grouped by ecosystem for readability. */
export const CONFIG_FILENAMES: readonly string[] = [
  // JavaScript / Node
  'package.json',
  '.yarnrc.yml',
  'pnpm-workspace.yaml',
  'tsconfig.json',
  'jsconfig.json',
  'turbo.json',
  'lerna.json',
  'nx.json',
  'babel.config.js',
  'vite.config.ts',
  'vite.config.js',
  'webpack.config.js',
  'rollup.config.js',
  'jest.config.js',
  'vitest.config.ts',
  'commitlint.config.js',
  'deno.json',
  'deno.jsonc',
  '.npmrc',
  '.nvmrc',
  '.node-version',
  '.prettierrc',
  '.prettierignore',
  '.eslintignore',
  '.stylelintrc',
  '.editorconfig',
  '.tool-versions',
  // Python
  'requirements.txt',
  'pyproject.toml',
  'setup.py',
  'setup.cfg',
  'Pipfile',
  'tox.ini',
  'environment.yml',
  '.python-version',
  // Ruby
  'Gemfile',
  '.ruby-version',
  // PHP
  'composer.json',
  // Rust
  'Cargo.toml',
  // Go
  'go.mod',
  'go.work',
  // Elixir / Erlang
  'mix.exs',
  'rebar.config',
  // Dart / Flutter
  'pubspec.yaml',
  // Scala / Clojure
  'build.sbt',
  'deps.edn',
  'project.clj',
  // Swift
  'Package.swift',
  // C / C++
  'CMakeLists.txt',
  'Makefile.am',
  'configure.ac',
  // Java / JVM (Maven + Gradle, incl. Kotlin DSL)
  'pom.xml',
  'build.gradle',
  'build.gradle.kts',
  'settings.gradle',
  'settings.gradle.kts',
  'gradle.properties',
  // .NET
  'global.json',
  'Directory.Build.props',
  'Directory.Build.targets',
  'packages.config',
  'nuget.config',
  // Containers / Infra
  'Dockerfile',
  'docker-compose.yml',
  'docker-compose.yaml',
  '.dockerignore',
  'Makefile',
  'Vagrantfile',
  'Chart.yaml',
  'Procfile',
  'Brewfile',
  'serverless.yml',
  'netlify.toml',
  'vercel.json',
  'renovate.json',
  // CI / tooling
  '.gitlab-ci.yml',
  '.travis.yml',
  'Jenkinsfile',
  // General / dotfiles
  '.gitignore',
  '.gitattributes',
  '.gitmodules',
  '.env',
  '.env.example',
] as const;

/** Fast lookup set for exact basename matches. */
const CONFIG_FILENAME_SET = new Set(CONFIG_FILENAMES);

/**
 * Dependency lock files — generated snapshots that pin a resolved dependency
 * tree. Distinct from config/manifests: they are not authored by hand and
 * rarely need to be read in full. Grouped by ecosystem.
 */
// Mirrors github-linguist/linguist `generated.rb` lockfile detection, the
// canonical reference for "generated, not authored" resolution snapshots.
export const LOCK_FILENAMES: readonly string[] = [
  // JavaScript / Node
  'package-lock.json',
  'npm-shrinkwrap.json',
  'yarn.lock',
  'pnpm-lock.yaml',
  'bun.lockb',
  'bun.lock',
  'deno.lock',
  // Python
  'Pipfile.lock',
  'poetry.lock',
  'uv.lock',
  'pdm.lock',
  'pixi.lock',
  // Ruby
  'Gemfile.lock',
  // PHP
  'composer.lock',
  // Rust
  'Cargo.lock',
  'Cargo.toml.orig',
  // Go (go.sum is the checksum/lock companion to go.mod)
  'go.sum',
  'Gopkg.lock',
  'glide.lock',
  // Swift
  'Package.resolved',
  // Dart / Flutter
  'pubspec.lock',
  // Nix / Bazel / Terraform / mise
  'flake.lock',
  'MODULE.bazel.lock',
  '.terraform.lock.hcl',
  'mise.lock',
] as const;

const LOCK_FILENAME_SET = new Set(LOCK_FILENAMES);

/**
 * Lock-file basename patterns for the many ecosystems whose lockfile simply
 * ends in `.lock` (Cargo.lock, Gemfile.lock, poetry.lock, yarn.lock, ...).
 */
const LOCK_FILENAME_PATTERNS: readonly RegExp[] = [/\.lock$/] as const;

/** Regex patterns for variadic / templated config filenames. */
export const CONFIG_FILENAME_PATTERNS: readonly RegExp[] = [
  /^\.eslintrc(\..+)?$/,
  /^\.prettierrc(\..+)?$/,
  /^\.babelrc(\..+)?$/,
  /^eslint\.config\.(c|m)?js$/,
  /^\.env(\..+)?$/, // .env.local, .env.production, ...
  /^tsconfig\..+\.json$/, // tsconfig.build.json, tsconfig.node.json
  /^\.stylelintrc(\..+)?$/,
  /^\.renovaterc(\..+)?$/,
  /\.csproj$/, // .NET C#
  /\.fsproj$/, // .NET F#
  /\.vbproj$/, // .NET VB
  /\.sln$/, // .NET solution
  /\.gemspec$/, // Ruby
  /\.tf(vars)?$/, // Terraform
] as const;

/** Extensions that indicate a structured config format. */
export const CONFIG_EXTENSIONS: readonly string[] = [
  '.toml',
  '.ini',
  '.cfg',
  '.conf',
  '.properties',
] as const;

const CONFIG_EXTENSION_SET = new Set(CONFIG_EXTENSIONS);

/** Documentation file extensions. */
// Prose types per github-linguist/linguist languages.yml (type: prose).
const DOC_EXTENSIONS = new Set<string>(
  '.md .mdx .markdown .rst .adoc .asciidoc .txt .text .rtf .org .textile .rdoc .pod .creole .wiki'.split(
    ' '
  )
);

/** Documentation basenames without (or regardless of) an extension. */
const DOC_BASENAMES = new Set<string>([
  'readme',
  'license',
  'licence',
  'copying',
  'notice',
  'changelog',
  'changes',
  'history',
  'authors',
  'contributors',
  'contributing',
  'code_of_conduct',
  'codeowners',
  'install',
  'citation',
]);

/**
 * Source-code extensions for the top popular languages. Kept deliberately
 * broad but bounded — anything not here (and not config/doc) is `other`.
 */
const CODE_EXTENSIONS = new Set<string>(
  // JS/TS · Python · Ruby · PHP · Go · Rust
  (
    '.js .jsx .mjs .cjs .ts .tsx .mts .cts .py .pyi .pyx .rb .rake .php .go .rs ' +
    // Java/JVM (Kotlin, Scala, Groovy, Clojure)
    '.java .kt .kts .scala .groovy .clj .cljs .cljc ' +
    // C / C++ / Objective-C
    '.c .h .cc .cpp .cxx .hpp .hh .m .mm ' +
    // C# / F# / VB · Swift · Dart
    '.cs .fs .vb .swift .dart ' +
    // Shell / scripting
    '.sh .bash .zsh .fish .ps1 .bat .cmd ' +
    // Web
    '.vue .svelte .css .scss .sass .less .html .htm ' +
    // Data/query languages
    '.sql .graphql .gql .proto ' +
    // Elixir/Erlang · Lua · Haskell · Perl · R · Julia
    '.ex .exs .erl .lua .hs .pl .pm .r .jl'
  ).split(' ')
);

/**
 * Coarse file-type buckets returned to fetch callers. Deliberately has no
 * `other`/`unknown` member: when a path can't be confidently placed in one of
 * these buckets, `classifyFileType` returns `undefined` and callers omit the
 * field entirely rather than emit a low-confidence guess.
 */
export type FileType = 'code' | 'config' | 'lock' | 'doc';

/** Extract the basename from a path (handles both `/` and `\` separators). */
function basename(filePath: string): string {
  const normalized = filePath.replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx === -1 ? normalized : normalized.slice(idx + 1);
}

/** Lowercased extension including the leading dot, or '' when none. */
function extname(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot).toLowerCase() : '';
}

/**
 * Returns true when the given path points to a common config / manifest file.
 *
 * @param filePath A file path or bare filename.
 */
export function isConfigFile(filePath: string): boolean {
  if (!filePath) return false;
  const name = basename(filePath);
  if (!name) return false;

  // 1. Exact basename match.
  if (CONFIG_FILENAME_SET.has(name)) return true;

  // 2. Regex patterns for variadic names.
  for (const pattern of CONFIG_FILENAME_PATTERNS) {
    if (pattern.test(name)) return true;
  }

  // 3. Config-ish extension match.
  const ext = extname(name);
  if (ext && CONFIG_EXTENSION_SET.has(ext)) return true;

  return false;
}

/**
 * Returns true when the path is a dependency lock file (a pinned, generated
 * resolution snapshot) rather than an authored config/manifest.
 *
 * @param filePath A file path or bare filename.
 */
export function isLockFile(filePath: string): boolean {
  if (!filePath) return false;
  const name = basename(filePath);
  if (!name) return false;

  if (LOCK_FILENAME_SET.has(name)) return true;
  for (const pattern of LOCK_FILENAME_PATTERNS) {
    if (pattern.test(name)) return true;
  }
  return false;
}

/** Returns true when the path is a documentation file. */
function isDocFile(name: string): boolean {
  const ext = extname(name);
  if (ext && DOC_EXTENSIONS.has(ext)) return true;
  // Extensionless / versioned doc files: README, LICENSE, CHANGELOG, ...
  const stem = (ext ? name.slice(0, name.length - ext.length) : name)
    .toLowerCase()
    .replace(/\..*$/, ''); // README.rst -> readme (handled above), LICENSE-MIT -> license
  const base = stem.split(/[-_.]/)[0] ?? stem;
  return DOC_BASENAMES.has(stem) || DOC_BASENAMES.has(base);
}

/** Returns true when the path is a source-code file. */
function isCodeFile(name: string): boolean {
  const ext = extname(name);
  return ext !== '' && CODE_EXTENSIONS.has(ext);
}

/**
 * Classify a path into a coarse bucket so agents can decide how to read it.
 *
 * Precedence (most specific first): lock → config → doc → code. Lock wins
 * first (a lockfile is never config/source). Config then wins over code/doc
 * because manifests like `vite.config.js` or `pyproject.toml` carry a
 * `.js`/`.toml` extension yet are configuration, not source.
 *
 * Returns `undefined` when the path matches none of the buckets — callers
 * MUST omit the field in that case rather than emit an uncertain guess.
 *
 * @param filePath A file path or bare filename.
 */
export function classifyFileType(filePath: string): FileType | undefined {
  if (!filePath) return undefined;
  const name = basename(filePath);
  if (!name) return undefined;

  if (isLockFile(name)) return 'lock';
  if (isConfigFile(name)) return 'config';
  if (isDocFile(name)) return 'doc';
  if (isCodeFile(name)) return 'code';
  return undefined;
}
