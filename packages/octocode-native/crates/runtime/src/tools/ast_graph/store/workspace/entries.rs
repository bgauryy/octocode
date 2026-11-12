//! Entrypoint inference: manifest entries, conventions and framework rules.
use super::*;

/// The convention that makes `file` an entry point, if any.
pub(super) fn convention_rule(
    file: &str,
    mains: &BTreeSet<&str>,
    marks: SourceMarks,
) -> Option<&'static str> {
    let segments = file.split('/').collect::<Vec<_>>();
    let name = segments.last().copied().unwrap_or_default();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    if JS_TS_EXTENSIONS.contains(&ext)
        && let Some(rule) = framework_rule(&segments, stem)
    {
        return Some(rule);
    }
    // Executables run as processes (`node x.mjs`, `./tool.py`), not
    // imported: a shebang or a bin/scripts directory makes a root.
    let in_script_dir = segments
        .iter()
        .rev()
        .skip(1)
        .any(|s| matches!(*s, "bin" | "scripts" | "script"));
    if in_script_dir
        && matches!(
            ext,
            "js" | "mjs" | "cjs" | "ts" | "mts" | "py" | "rb" | "sh"
        )
    {
        return Some("script directory");
    }
    if matches!(ext, "js" | "mjs" | "cjs" | "ts" | "mts" | "py") && marks.shebang {
        return Some("executable script");
    }
    // C-family and assembly sources are linker inputs, never `#include`d:
    // each translation unit is a root, so only headers can be unreachable.
    if matches!(
        ext,
        "c" | "cc" | "cpp" | "cxx" | "cu" | "m" | "mm" | "s" | "asm" | "S"
    ) {
        return Some("compilation unit");
    }
    let rule = match ext {
        "go" => "go package main",
        "py" if matches!(name, "__main__.py" | "manage.py" | "wsgi.py" | "asgi.py") => {
            return Some("python entry name");
        }
        "py" => "python __main__ guard",
        "java" | "kt" | "scala" | "cs" => {
            return mains.contains(file).then_some(if ext == "cs" {
                "dotnet Main"
            } else {
                "jvm main"
            });
        }
        _ => return None,
    };
    marks.main_marker.then_some(rule)
}

/// Framework route and convention files (Next, Remix, SvelteKit, Nuxt).
pub(super) fn framework_rule(segments: &[&str], stem: &str) -> Option<&'static str> {
    const ROUTE_FILES: &[&str] = &[
        "page",
        "layout",
        "route",
        "loading",
        "error",
        "not-found",
        "template",
        "default",
        "global-error",
        "head",
        "opengraph-image",
        "sitemap",
        "robots",
    ];
    let under = |dir: &str| segments.iter().rev().skip(1).any(|s| *s == dir);
    if under("pages") {
        Some("framework route (pages/)")
    } else if under("app") && ROUTE_FILES.contains(&stem) {
        Some("framework route (app/)")
    } else if under("routes") || stem == "+page" || stem == "+server" || stem.starts_with("+layout")
    {
        Some("framework route (routes/)")
    } else if matches!(
        stem,
        "middleware"
            | "instrumentation"
            | "entry.client"
            | "entry.server"
            | "root"
            | "_app"
            | "_document"
    ) {
        Some("framework convention")
    } else if under("api") && under("server") {
        Some("framework route (server/api)")
    } else {
        None
    }
}

/// The rule of a target a Cargo manifest builds, by declaration or Cargo's
/// default paths.
const CARGO_TARGET: &str = "cargo target";
/// The rule of a file `package.json` names as its package's entry.
const PACKAGE_ENTRY: &str = "package.json";

/// Whether `rule` is a manifest's own declaration of a target it builds or
/// exports: such a file is compiled or loaded as the package's code, so a
/// role guessed from its spelling (a `build.rs` read as a build-tool
/// config) does not hold.
pub(in super::super) fn declared_target(rule: &str) -> bool {
    matches!(rule, CARGO_TARGET | PACKAGE_ENTRY)
}

impl Workspace {
    pub(super) fn add_entry(&mut self, file: &str, rule: &'static str, known: &HashSet<&str>) {
        let file = file.trim_start_matches("./").to_owned();
        if known.contains(file.as_str()) {
            self.entries.entry(file).or_insert(rule);
            return;
        }
        // Build output → source (`dist/index.js` → `src/index.ts`).
        let (stem, _) = file.rsplit_once('.').unwrap_or((&file, ""));
        let mut stems = vec![stem.to_owned()];
        for out in BUILD_OUTPUT_DIRS.iter().map(|dir| format!("{dir}/")) {
            if let Some(pos) = stem.find(&out)
                && (pos == 0 || stem.as_bytes()[pos - 1] == b'/')
            {
                stems.push(format!("{}src/{}", &stem[..pos], &stem[pos + out.len()..]));
                stems.push(format!("{}{}", &stem[..pos], &stem[pos + out.len()..]));
            }
        }
        for candidate in &stems {
            for ext in JS_TS_EXTENSIONS {
                let path = format!("{candidate}.{ext}");
                if known.contains(path.as_str()) {
                    self.entries.entry(path).or_insert(rule);
                    return;
                }
            }
        }
    }

    pub(super) fn npm_entries(
        &mut self,
        root: &Path,
        dir: &str,
        manifest: &Value,
        known: &HashSet<&str>,
    ) {
        fn leaves(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::String(s) => out.push(s.clone()),
                Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
                Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
                _ => {}
            }
        }
        let mut targets = Vec::new();
        for field in ["main", "module", "browser", "bin", "exports", "source"] {
            leaves(&manifest[field], &mut targets);
        }
        for target in targets
            .iter()
            .filter(|t| !t.contains('*') && !super::super::super::js_paths::is_declaration(t))
        {
            self.add_entry(&join(dir, target), PACKAGE_ENTRY, known);
        }
        for script in manifest["scripts"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(_, v)| v.as_str())
        {
            for token in script.split_whitespace() {
                let token = token.trim_matches(['"', '\'']);
                if token
                    .rsplit_once('.')
                    .is_some_and(|(_, ext)| JS_TS_EXTENSIONS.contains(&ext))
                    && root.join(dir).join(token).is_file()
                {
                    self.add_entry(&join(dir, token), "package.json scripts", known);
                }
            }
        }
        for name in ["index", "main", "cli", "server", "app"] {
            for base in ["", "src/"] {
                for ext in JS_TS_EXTENSIONS {
                    let candidate = join(dir, &format!("{base}{name}.{ext}"));
                    if known.contains(candidate.as_str()) {
                        self.entries
                            .entry(candidate)
                            .or_insert("default entry name");
                    }
                }
            }
        }
    }

    pub(super) fn cargo_entries(
        &mut self,
        dir: &str,
        manifest: &toml::Table,
        files: &[&str],
        known: &HashSet<&str>,
    ) {
        for file in ["src/main.rs", "src/lib.rs"] {
            self.add_entry(&join(dir, file), CARGO_TARGET, known);
        }
        // The build script: `package.build` names it, `false` turns it off,
        // else Cargo runs `build.rs` beside the manifest.
        let build = manifest
            .get("package")
            .and_then(|package| package.get("build"));
        match build {
            Some(toml::Value::String(path)) => {
                self.add_entry(&join(dir, path), CARGO_TARGET, known);
            }
            Some(toml::Value::Boolean(false)) => {}
            _ => self.add_entry(&join(dir, "build.rs"), CARGO_TARGET, known),
        }
        for section in ["bin", "example", "test", "bench"] {
            for target in manifest
                .get(section)
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(path) = target.get("path").and_then(|p| p.as_str()) {
                    self.add_entry(&join(dir, path), CARGO_TARGET, known);
                }
            }
        }
        if let Some(path) = manifest
            .get("lib")
            .and_then(|l| l.get("path"))
            .and_then(|p| p.as_str())
        {
            self.add_entry(&join(dir, path), CARGO_TARGET, known);
        }
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        for file in files {
            let Some(rest) = file.strip_prefix(&prefix) else {
                continue;
            };
            let auto = ["src/bin/", "examples/", "tests/", "benches/"]
                .iter()
                .any(|d| {
                    rest.strip_prefix(d)
                        .is_some_and(|tail| !tail.contains('/') || tail.ends_with("/main.rs"))
                });
            if auto && rest.ends_with(".rs") {
                self.entries
                    .entry((*file).to_owned())
                    .or_insert("cargo auto-target");
            }
        }
    }

    /// Framework routing conventions and language-level `main`s.
    pub(super) fn convention_entries(
        &mut self,
        files: &[&str],
        mains: &BTreeSet<&str>,
        marks: &dyn Fn(&str) -> SourceMarks,
    ) {
        for file in files {
            if let Some(rule) = convention_rule(file, mains, marks(file)) {
                self.entries.entry((*file).to_owned()).or_insert(rule);
            }
        }
    }
}
