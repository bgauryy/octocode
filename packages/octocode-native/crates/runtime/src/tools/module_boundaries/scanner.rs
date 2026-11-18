//! The import scanner: every module path a production source names, resolved
//! to an absolute path from the module that names it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One code token the import scanner reads: an identifier, `::`, or one
/// punctuation character.
fn tokens(code: &str) -> Vec<&str> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_whitespace() {
            at += 1;
        } else if byte.is_ascii_alphanumeric() || byte == b'_' {
            let end = bytes[at..]
                .iter()
                .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
                .map_or(bytes.len(), |len| at + len);
            out.push(&code[at..end]);
            at = end;
        } else if bytes[at..].starts_with(b"::") {
            out.push("::");
            at += 2;
        } else {
            let width = code[at..].chars().next().map_or(1, char::len_utf8);
            out.push(&code[at..at + width]);
            at += width;
        }
    }
    out
}

fn is_ident(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
}

/// `path` as an absolute module path seen from `module`: `crate::…`,
/// `self::…`, `super::…` (any depth), or a name a `use` bound. `None` for
/// a path rooted elsewhere (an extern crate, a local item).
fn resolve(
    module: &[String],
    path: &[String],
    bindings: &BTreeMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    let (first, rest) = path.split_first()?;
    let mut base = match first.as_str() {
        "crate" => Vec::new(),
        "self" => module.to_vec(),
        "super" => {
            let up = 1 + rest
                .iter()
                .take_while(|segment| *segment == "super")
                .count();
            let base = module[..module.len().checked_sub(up)?].to_vec();
            let mut resolved = base;
            resolved.extend(rest.iter().skip(up - 1).cloned());
            return Some(resolved);
        }
        name => bindings.get(name)?.clone(),
    };
    base.extend(rest.iter().cloned());
    Some(base)
}

/// One `use` tree from `tokens[*at]` under `prefix`: every leaf as
/// `(bound name, path)`; a glob binds nothing.
fn use_tree(
    tokens: &[&str],
    at: &mut usize,
    prefix: Vec<String>,
    leaves: &mut Vec<(Option<String>, Vec<String>)>,
) {
    let mut path = prefix;
    let grouped = !path.is_empty();
    while let Some(token) = tokens.get(*at).copied() {
        match token {
            "{" => {
                *at += 1;
                while tokens.get(*at).is_some_and(|token| *token != "}") {
                    use_tree(tokens, at, path.clone(), leaves);
                    if tokens.get(*at) == Some(&",") {
                        *at += 1;
                    }
                }
                *at += 1;
                return;
            }
            "*" => {
                *at += 1;
                leaves.push((None, path));
                return;
            }
            "self" if grouped && tokens.get(*at + 1) != Some(&"::") => {
                *at += 1;
            }
            token if is_ident(token) => {
                path.push(token.to_owned());
                *at += 1;
                if tokens.get(*at) == Some(&"::") {
                    *at += 1;
                    continue;
                }
            }
            _ => return,
        }
        let bound = if tokens.get(*at) == Some(&"as") {
            *at += 2;
            tokens.get(*at - 1).map(|alias| (*alias).to_owned())
        } else {
            path.last().cloned()
        };
        leaves.push((bound, path));
        return;
    }
}

/// Every absolute module path `code` (production code, no comments or
/// literals) names from `module`: each `use` leaf (groups, `self`, `super`,
/// aliases and globs included) and each inline path rooted at `crate`,
/// `self`, `super` or a `use`-bound name.
pub(super) fn named_paths(code: &str, module: &[String]) -> Vec<Vec<String>> {
    let tokens = tokens(code);
    let mut bindings = BTreeMap::new();
    let mut found = Vec::new();
    let mut in_use = vec![false; tokens.len()];
    let mut at = 0;
    while at < tokens.len() {
        if tokens[at] == "use" && (at == 0 || tokens[at - 1] != "::") {
            let start = at;
            at += 1;
            if tokens.get(at) == Some(&"::") {
                at += 1;
            }
            let mut leaves = Vec::new();
            use_tree(&tokens, &mut at, Vec::new(), &mut leaves);
            for (bound, path) in leaves {
                let resolved = resolve(module, &path, &bindings).unwrap_or(path);
                if let Some(bound) = bound.filter(|bound| bound != "_") {
                    bindings.insert(bound, resolved.clone());
                }
                found.push(resolved);
            }
            in_use[start..at.min(tokens.len())].fill(true);
        } else {
            at += 1;
        }
    }
    let mut at = 0;
    while at < tokens.len() {
        let starts = !in_use[at]
            && is_ident(tokens[at])
            && tokens.get(at + 1) == Some(&"::")
            && (at == 0 || tokens[at - 1] != "::");
        if !starts {
            at += 1;
            continue;
        }
        let mut path = vec![tokens[at].to_owned()];
        at += 1;
        while tokens.get(at) == Some(&"::") && tokens.get(at + 1).is_some_and(|t| is_ident(t)) {
            path.push(tokens[at + 1].to_owned());
            at += 2;
        }
        found.extend(resolve(module, &path, &bindings));
    }
    found
}

/// The top-level modules (the first segment of each absolute path) a
/// production source of `module` reaches; a path rooted elsewhere names its
/// extern crate.
pub(super) fn imported_modules(source: &str, module: &[String]) -> BTreeSet<String> {
    let code = octocode_runtime_base::test_support::code_only(
        &octocode_runtime_base::test_support::production_text(source),
    );
    named_paths(&code, module)
        .into_iter()
        .filter_map(|path| path.into_iter().next())
        .collect()
}

/// The items a module file defines at its top level (`fn`, `const`,
/// `static`, `struct`, `enum`, `trait`, `type`), from its production code:
/// the names a path into that module may end on instead of a submodule.
pub(super) fn root_items(source: &str) -> BTreeSet<String> {
    let code = octocode_runtime_base::test_support::code_only(
        &octocode_runtime_base::test_support::production_text(source),
    );
    let tokens = tokens(&code);
    let mut depth = 0usize;
    let mut items = BTreeSet::new();
    for (at, token) in tokens.iter().enumerate() {
        match *token {
            "{" => depth += 1,
            "}" => depth = depth.saturating_sub(1),
            "fn" | "const" | "static" | "struct" | "enum" | "trait" | "type" if depth == 0 => {
                // `const fn f` is named by its `fn`; `static mut X` one token on.
                let name = match tokens.get(at + 1).copied() {
                    Some("fn") => None,
                    Some("mut") => tokens.get(at + 2).copied(),
                    name => name,
                };
                if let Some(name) = name.filter(|name| is_ident(name)) {
                    items.insert(name.to_owned());
                }
            }
            _ => {}
        }
    }
    items
}

/// The module path of a source file under its crate's `src`: `x/y.rs` is
/// `x::y`, `x.rs` is `x`, and the crate root `lib.rs` is the empty path.
pub(super) fn module_of(src: &Path, path: &Path) -> Vec<String> {
    let relative = path.strip_prefix(src).unwrap_or(Path::new(""));
    if relative == Path::new("lib.rs") {
        return Vec::new();
    }
    relative
        .with_extension("")
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect()
}

/// Every spelling Rust accepts for a path into another module resolves to
/// it: absolute and grouped `crate` paths, `super` and `self` at any depth,
/// a bound module and its alias, and inline paths. Comments, literals and
/// test-only items name nothing.
#[test]
fn the_scanner_resolves_every_import_form() {
    let src = Path::new("/src");
    let module = |file: &str| module_of(src, &src.join(file));
    let reached = |file: &str, source: &str| {
        imported_modules(source, &module(file))
            .into_iter()
            .collect::<Vec<_>>()
    };
    assert!(module("lib.rs").is_empty());
    assert_eq!(module("local_search.rs"), ["local_search"]);
    assert_eq!(module("local_search/a/b.rs"), ["local_search", "a", "b"]);
    for (file, source) in [
        ("local_search.rs", "use super::local_fetch::A;"),
        ("local_search.rs", "use crate::{local_fetch::A};"),
        ("local_search.rs", "use crate::{local_fetch::{A, B}};"),
        ("local_search.rs", "fn f() { super::local_fetch::x(); }"),
        ("local_search.rs", "fn f() { crate::local_fetch::x(); }"),
        ("local_search/a.rs", "use super::super::local_fetch::A;"),
        (
            "local_search/a/b.rs",
            "use super::super::super::local_fetch::{self};",
        ),
        (
            "local_search.rs",
            "use crate::local_fetch;\nfn f() { local_fetch::x(); }",
        ),
        (
            "local_search.rs",
            "use crate::local_fetch as t;\nfn f() { t::x(); }",
        ),
        (
            "local_search.rs",
            "use crate::{local_fetch::{self as t}};\nfn f() { t::x(); }",
        ),
        ("local_search.rs", "pub(crate) use crate::local_fetch::*;"),
        (
            "local_search/a.rs",
            "use super::super::{local_fetch as fetch};\nfn f() { fetch::x(); }",
        ),
    ] {
        assert_eq!(reached(file, source), ["local_fetch"], "{file}: {source}");
    }
    assert_eq!(
        reached(
            "local_search/a.rs",
            "use self::inner::A;\nuse super::helper;"
        ),
        ["local_search"]
    );
    assert_eq!(
        reached(
            "local_search.rs",
            "use crate::{id::ToolId, local_fetch::{A, B}, result};\n\
             fn f() { crate::gh_shared::x(); }"
        ),
        ["gh_shared", "id", "local_fetch", "result"]
    );
    let nothing = "// crate::lsp_search::ignored\n\
                   /* use super::clasify::x; */\n\
                   const S: &str = \"crate::clasify::y\";\n\
                   #[cfg(test)]\nmod tests { use crate::clasify::t; }\n\
                   #[cfg(test)]\nfn helper() { super::clasify::z(); }\n";
    assert!(reached("local_search.rs", nothing).is_empty());
    assert_eq!(
        root_items(
            "pub(crate) mod a;\npub(crate) const N: usize = 5;\nconst fn f() {}\n\
             static mut S: u8 = 0;\nfn g() { fn inner() {} }\n\
             #[cfg(test)]\nfn only_tests() {}\n"
        )
        .into_iter()
        .collect::<Vec<_>>(),
        ["N", "S", "f", "g"]
    );
}
