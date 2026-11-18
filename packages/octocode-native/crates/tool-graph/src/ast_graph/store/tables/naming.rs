//! Naming helpers: package names, unparsed targets, languages, ecosystems
//! and callee splitting.
use super::*;

/// Package identity for an external import specifier, by ecosystem rules.
pub(super) fn package_name(specifier: &str, language: &str) -> Option<String> {
    let spec = specifier.trim();
    if spec.is_empty() || spec.starts_with('.') || spec.starts_with('/') {
        return None;
    }
    let name = match language {
        "rust" => spec.trim_start_matches("::").split("::").next()?.to_owned(),
        "python" => spec.split('.').next()?.to_owned(),
        "go" => {
            let parts = spec.split('/').collect::<Vec<_>>();
            if parts[0].contains('.') && parts.len() >= 3 {
                parts[..3].join("/")
            } else {
                spec.to_owned()
            }
        }
        "java" | "scala" => match spec.rsplit_once('.') {
            Some((package, _)) => package.to_owned(),
            None => spec.to_owned(),
        },
        "c" | "cpp" | "cuda" => spec.trim_matches(['<', '>', '"']).to_owned(),
        _ => {
            if let Some(rest) = spec.strip_prefix('@') {
                let mut parts = rest.splitn(3, '/');
                match (parts.next(), parts.next()) {
                    (Some(scope), Some(name)) => format!("@{scope}/{name}"),
                    _ => spec.to_owned(),
                }
            } else {
                spec.split('/').next()?.to_owned()
            }
        }
    };
    (!name.is_empty()).then_some(name)
}

/// The unparsed file a relative specifier from `importer` names, probing
/// the usual source extensions and `index` files.
pub(super) fn unparsed_target<'a>(
    importer: &str,
    spec: &str,
    unparsed: &BTreeSet<&'a str>,
) -> Option<&'a str> {
    if !spec.starts_with('.') {
        return None;
    }
    let mut parts = importer.split('/').collect::<Vec<_>>();
    parts.pop();
    for segment in spec.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    let base = parts.join("/");
    const SUFFIXES: &[&str] = &[
        "",
        ".js",
        ".mjs",
        ".cjs",
        ".ts",
        ".tsx",
        ".jsx",
        "/index.js",
        "/index.ts",
    ];
    SUFFIXES.iter().find_map(|suffix| {
        let candidate = format!("{base}{suffix}");
        unparsed.get(candidate.as_str()).copied()
    })
}

pub(super) fn ecosystem(language: &str) -> &'static str {
    match language {
        "javascript" | "typescript" | "tsx" => "npm",
        "csharp" => "nuget",
        "python" => "pypi",
        "rust" => "cargo",
        "go" => "go",
        "java" | "scala" => "maven",
        "c" | "cpp" | "cuda" => "system",
        _ => "other",
    }
}

/// The languages whose declarations can bind each other's names without an
/// import: one package ecosystem (JS/TS, C/C++/CUDA, Java/Scala), else the
/// language alone.
pub(super) fn link_family(language: &str) -> &str {
    match ecosystem(language) {
        "other" => language,
        family => family,
    }
}

/// `(qualifier, name)` of a callee: `this.run` → `("this", "run")`,
/// `Self::new` → `("Self", "new")`, `run` → `("", "run")`.
pub(super) fn split_callee(callee: &str) -> (&str, &str) {
    match callee.rfind(['.', ':']) {
        Some(at) => {
            let name = &callee[at + 1..];
            let qualifier = callee[..at].trim_end_matches([':', '.']);
            (qualifier, name)
        }
        None => ("", callee),
    }
}

pub(super) fn first_segment(qualifier: &str) -> &str {
    qualifier
        .split(['.', ':'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(qualifier)
}
