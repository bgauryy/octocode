#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileType {
    Code,
    Config,
    Lock,
    Doc,
}

const CONFIG_NAMES: &[&str] = &[
    "package.json",
    ".yarnrc.yml",
    "pnpm-workspace.yaml",
    "tsconfig.json",
    "jsconfig.json",
    "turbo.json",
    "lerna.json",
    "nx.json",
    "babel.config.js",
    "vite.config.ts",
    "vite.config.js",
    "webpack.config.js",
    "rollup.config.js",
    "jest.config.js",
    "vitest.config.ts",
    "commitlint.config.js",
    "deno.json",
    "deno.jsonc",
    ".npmrc",
    ".nvmrc",
    ".node-version",
    ".prettierrc",
    ".prettierignore",
    ".eslintignore",
    ".stylelintrc",
    ".editorconfig",
    ".tool-versions",
    "requirements.txt",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "Pipfile",
    "tox.ini",
    "environment.yml",
    ".python-version",
    "Gemfile",
    ".ruby-version",
    "composer.json",
    "Cargo.toml",
    "go.mod",
    "go.work",
    "mix.exs",
    "rebar.config",
    "pubspec.yaml",
    "build.sbt",
    "deps.edn",
    "project.clj",
    "Package.swift",
    "CMakeLists.txt",
    "Makefile.am",
    "configure.ac",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle.properties",
    "global.json",
    "Directory.Build.props",
    "Directory.Build.targets",
    "packages.config",
    "nuget.config",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    ".dockerignore",
    "Makefile",
    "Vagrantfile",
    "Chart.yaml",
    "Procfile",
    "Brewfile",
    "serverless.yml",
    "netlify.toml",
    "vercel.json",
    "renovate.json",
    ".gitlab-ci.yml",
    ".travis.yml",
    "Jenkinsfile",
    ".gitignore",
    ".gitattributes",
    ".gitmodules",
    ".env",
    ".env.example",
];
const LOCK_NAMES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "bun.lock",
    "deno.lock",
    "Pipfile.lock",
    "poetry.lock",
    "uv.lock",
    "pdm.lock",
    "pixi.lock",
    "Gemfile.lock",
    "composer.lock",
    "Cargo.lock",
    "Cargo.toml.orig",
    "go.sum",
    "Gopkg.lock",
    "glide.lock",
    "Package.resolved",
    "pubspec.lock",
    "flake.lock",
    "MODULE.bazel.lock",
    ".terraform.lock.hcl",
    "mise.lock",
];
const CONFIG_EXTENSIONS: &[&str] = &[".toml", ".ini", ".cfg", ".conf", ".properties"];
const DOC_EXTENSIONS: &[&str] = &[
    ".md",
    ".mdx",
    ".markdown",
    ".rst",
    ".adoc",
    ".asciidoc",
    ".txt",
    ".text",
    ".rtf",
    ".org",
    ".textile",
    ".rdoc",
    ".pod",
    ".creole",
    ".wiki",
];
const DOC_NAMES: &[&str] = &[
    "readme",
    "license",
    "licence",
    "copying",
    "notice",
    "changelog",
    "changes",
    "history",
    "authors",
    "contributors",
    "contributing",
    "code_of_conduct",
    "codeowners",
    "install",
    "citation",
];
const CODE_EXTENSIONS: &[&str] = &[
    ".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts", ".py", ".pyi", ".pyx", ".rb",
    ".rake", ".php", ".go", ".rs", ".java", ".kt", ".kts", ".scala", ".groovy", ".clj", ".cljs",
    ".cljc", ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".m", ".mm", ".cs", ".fs", ".vb",
    ".swift", ".dart", ".sh", ".bash", ".zsh", ".fish", ".ps1", ".bat", ".cmd", ".vue", ".svelte",
    ".css", ".scss", ".sass", ".less", ".html", ".htm", ".sql", ".graphql", ".gql", ".proto",
    ".ex", ".exs", ".erl", ".hs", ".pl", ".pm", ".r", ".jl",
];

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or("")
}
fn extension(name: &str) -> &str {
    name.rfind('.')
        .filter(|&i| i > 0)
        .map_or("", |i| &name[i..])
}
fn extension_is(name: &str, values: &[&str]) -> bool {
    let ext = extension(name);
    values.iter().any(|value| ext.eq_ignore_ascii_case(value))
}
fn wrapped_pattern(name: &str, stem: &str) -> bool {
    name == stem || name.starts_with(&format!("{stem}."))
}

pub fn is_config_file(path: &str) -> bool {
    let name = basename(path);
    if name.is_empty() {
        return false;
    }
    if CONFIG_NAMES.contains(&name) {
        return true;
    }
    let variable = wrapped_pattern(name, ".eslintrc")
        || wrapped_pattern(name, ".prettierrc")
        || wrapped_pattern(name, ".babelrc")
        || wrapped_pattern(name, ".stylelintrc")
        || wrapped_pattern(name, ".renovaterc")
        || wrapped_pattern(name, ".env")
        || (name.starts_with("eslint.config.") && matches!(&name[14..], "js" | "cjs" | "mjs"))
        || (name.starts_with("tsconfig.") && name.ends_with(".json"))
        || [".csproj", ".fsproj", ".vbproj", ".sln", ".gemspec"]
            .iter()
            .any(|s| name.ends_with(s))
        || name.ends_with(".tf")
        || name.ends_with(".tfvars");
    variable || extension_is(name, CONFIG_EXTENSIONS)
}

pub fn is_lock_file(path: &str) -> bool {
    let name = basename(path);
    !name.is_empty() && (LOCK_NAMES.contains(&name) || name.ends_with(".lock"))
}

fn is_doc(name: &str) -> bool {
    if extension_is(name, DOC_EXTENSIONS) {
        return true;
    }
    if extension_is(name, CODE_EXTENSIONS) || extension_is(name, CONFIG_EXTENSIONS) {
        return false;
    }
    let ext = extension(name);
    let stem = if ext.is_empty() {
        name
    } else {
        &name[..name.len() - ext.len()]
    };
    let lowered = stem.to_ascii_lowercase();
    let stem = lowered.split('.').next().unwrap_or(&lowered);
    let base = stem.split(['-', '_', '.']).next().unwrap_or(stem);
    DOC_NAMES.contains(&stem) || DOC_NAMES.contains(&base)
}

pub fn classify_file_type(path: &str) -> Option<FileType> {
    let name = basename(path);
    if name.is_empty() {
        return None;
    }
    if is_lock_file(name) {
        Some(FileType::Lock)
    } else if is_config_file(name) {
        Some(FileType::Config)
    } else if is_doc(name) {
        Some(FileType::Doc)
    } else if extension_is(name, CODE_EXTENSIONS) {
        Some(FileType::Code)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_precedence_and_cross_platform_paths() {
        assert_eq!(
            classify_file_type(r"C:\\repo\\Cargo.lock"),
            Some(FileType::Lock)
        );
        assert_eq!(
            classify_file_type("src/vite.config.js"),
            Some(FileType::Config)
        );
        assert_eq!(classify_file_type("README.md"), Some(FileType::Doc));
        assert_eq!(classify_file_type("install.sh"), Some(FileType::Code));
        assert_eq!(classify_file_type("asset.bin"), None);
    }
    #[test]
    fn covers_variadic_config_and_docs() {
        for name in [
            ".eslintrc.cjs",
            ".env.production",
            "tsconfig.build.json",
            "a.csproj",
            "main.tfvars",
            "service.CONF",
        ] {
            assert!(is_config_file(name), "{name}");
        }
        for name in ["LICENSE-MIT", "CHANGELOG.1", "CONTRIBUTING", "guide.rst"] {
            assert_eq!(classify_file_type(name), Some(FileType::Doc), "{name}");
        }
    }
}
