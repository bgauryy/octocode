//! C# and Scala namespace imports as file dependencies.
//!
//! A C# `using N;` or a Scala wildcard import of a package (`import a.b._`,
//! `import a.b.*`) brings the types of `N` into scope without naming a
//! file. A type of `N` the file then uses (a call, a receiver type, a base
//! type) is a dependency on the file declaring it, as a Java class depends on
//! a class of its package (`java-same-package`): a `namespace-use` edge that
//! cites the import line. A namespace holds, in C#, the types its `namespace`
//! declarations enclose directly; in Scala, the top-level types of the files
//! whose directory ends in the package path, as Scala call linking reads a
//! package. A name the file declares itself, or (C#) one its own namespace
//! declares, binds there; a name two imported namespaces declare is
//! ambiguous: neither adds an edge.
use super::*;
use std::collections::{HashMap, HashSet};

/// The edge kind of a type a namespace import brings into scope.
pub(super) const NAMESPACE_USE: &str = "namespace-use";

/// The grammars whose namespace imports this pass links.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grammar {
    CSharp,
    Scala,
}

fn grammar(file: &str, facts: &FileFacts) -> Option<Grammar> {
    match super::super::languages::canonical_language(file, &facts.language).as_str() {
        "csharp" => Some(Grammar::CSharp),
        "scala" => Some(Grammar::Scala),
        _ => None,
    }
}

const TYPE_KINDS: &[&str] = &[
    "class",
    "interface",
    "struct",
    "enum",
    "type",
    "trait",
    "object",
    "module",
    "record",
];

/// Where each type a namespace holds is declared.
#[derive(Default)]
struct Declared<'b> {
    /// C#: `(namespace, type name)` → declaring files.
    csharp: HashMap<(String, &'b str), Vec<&'b str>>,
    /// Scala: type name → `(directory, file)` of each top-level declaration.
    scala: HashMap<&'b str, Vec<(&'b str, &'b str)>>,
}

impl<'b> Declared<'b> {
    fn of(b: &'b BuiltGraph) -> Self {
        let mut declared = Self::default();
        for (file, facts) in &b.facts {
            let Some(grammar) = grammar(file, facts) else {
                continue;
            };
            let by_id = facts
                .declarations
                .iter()
                .map(|decl| (decl.id.as_str(), decl))
                .collect::<HashMap<_, _>>();
            // A file-scoped `namespace N;` encloses no declaration node: it
            // holds the file's top-level types.
            let file_scoped = facts
                .declarations
                .iter()
                .find(|decl| {
                    decl.kind == "namespace" && decl.parent.is_none() && decl.end_line == decl.line
                })
                .map(|decl| decl.name.clone());
            for decl in facts
                .declarations
                .iter()
                .filter(|decl| TYPE_KINDS.contains(&decl.kind.as_str()))
            {
                match grammar {
                    Grammar::CSharp => {
                        let namespace = match decl.parent {
                            None => file_scoped.clone(),
                            Some(_) => enclosing_namespace(decl, &by_id),
                        };
                        if let Some(namespace) = namespace {
                            declared
                                .csharp
                                .entry((namespace, decl.name.as_str()))
                                .or_default()
                                .push(file);
                        }
                    }
                    Grammar::Scala if decl.parent.is_none() => declared
                        .scala
                        .entry(decl.name.as_str())
                        .or_default()
                        .push((dirname(file), file)),
                    Grammar::Scala => {}
                }
            }
        }
        declared
    }

    /// The files declaring `name` in the namespace or package `path`.
    fn files(&self, grammar: Grammar, path: &str, name: &str) -> Vec<&'b str> {
        match grammar {
            Grammar::CSharp => self
                .csharp
                .get(&(path.to_owned(), name))
                .cloned()
                .unwrap_or_default(),
            Grammar::Scala => {
                let package = path.replace('.', "/");
                let suffix = format!("/{package}");
                self.scala
                    .get(name)
                    .into_iter()
                    .flatten()
                    .filter(|(dir, _)| *dir == package || dir.ends_with(&suffix))
                    .map(|(_, file)| *file)
                    .collect()
            }
        }
    }
}

/// The dotted namespace that directly encloses `decl` when every ancestor
/// is a `namespace` declaration; `None` for a nested or global type.
fn enclosing_namespace(decl: &Declaration, by_id: &HashMap<&str, &Declaration>) -> Option<String> {
    let mut names = Vec::new();
    let mut parent = decl.parent.as_deref();
    while let Some(id) = parent {
        let ancestor = by_id.get(id)?;
        if ancestor.kind != "namespace" {
            return None;
        }
        names.push(ancestor.name.as_str());
        parent = ancestor.parent.as_deref();
    }
    names.reverse();
    (!names.is_empty()).then(|| names.join("."))
}

/// The namespace or package a namespace import brings into scope: a C#
/// `using N;` (no alias), or a Scala wildcard (`a.b._`, `a.b.*`).
fn imported_namespace(grammar: Grammar, import: &Import) -> Option<&str> {
    let specifier = import.specifier.trim();
    match grammar {
        Grammar::CSharp => (import.local_name.is_none()
            && !specifier.is_empty()
            && !specifier.contains(['=', '<', ' ']))
        .then_some(specifier),
        Grammar::Scala => specifier
            .strip_suffix("._")
            .or_else(|| specifier.strip_suffix(".*"))
            .filter(|path| !path.is_empty() && !path.contains(['{', '}', ' '])),
    }
}

/// Every type name `facts` uses, with the line of its first use: a call's
/// head (`Chain` of `Chain.one()`), a receiver's type and a base type.
fn used_names(facts: &FileFacts) -> BTreeMap<&str, u32> {
    fn head(text: &str) -> &str {
        text.split(['.', '(', '<', '[']).next().unwrap_or_default()
    }
    let mut named = BTreeMap::<&str, u32>::new();
    let uses = facts
        .calls
        .iter()
        .flat_map(|call| {
            [
                Some(head(&call.callee)),
                call.receiver_type.as_deref().map(head),
            ]
            .into_iter()
            .flatten()
            .map(|name| (name, call.line))
        })
        .chain(
            facts
                .heritage
                .iter()
                .map(|heritage| (head(&heritage.target), heritage.line)),
        );
    for (name, line) in uses.filter(|(name, _)| name.starts_with(char::is_uppercase)) {
        let first = named.entry(name).or_insert(line);
        *first = (*first).min(line);
    }
    named
}

/// Links each C# and Scala file to the files declaring the types its
/// namespace imports bring in and it uses.
pub(super) fn link_namespace_uses(b: &mut BuiltGraph) {
    let declared = Declared::of(b);
    let mut gained = Vec::<(String, String, u32)>::new();
    for (file, facts) in &b.facts {
        let Some(grammar) = grammar(file, facts) else {
            continue;
        };
        let imports = facts
            .imports
            .iter()
            .filter_map(|import| Some((imported_namespace(grammar, import)?, import.line)))
            .collect::<Vec<_>>();
        if imports.is_empty() {
            continue;
        }
        let own_names = facts
            .declarations
            .iter()
            .map(|decl| decl.name.as_str())
            .collect::<HashSet<_>>();
        // C# binds a name in the file's own namespace chain before a `using`.
        let own_namespaces = facts
            .declarations
            .iter()
            .filter(|decl| decl.kind == "namespace")
            .flat_map(|decl| {
                let parts = decl.name.split('.').collect::<Vec<_>>();
                (1..=parts.len()).map(move |n| parts[..n].join("."))
            })
            .collect::<HashSet<_>>();
        for name in used_names(facts).into_keys() {
            if own_names.contains(name)
                || (grammar == Grammar::CSharp
                    && own_namespaces
                        .iter()
                        .any(|namespace| !declared.files(grammar, namespace, name).is_empty()))
            {
                continue;
            }
            let mut found = imports.iter().filter_map(|(path, line)| {
                let files = declared.files(grammar, path, name);
                (!files.is_empty()).then_some((files, *line))
            });
            let (Some((files, line)), None) = (found.next(), found.next()) else {
                continue;
            };
            gained.extend(
                files
                    .into_iter()
                    .filter(|target| target != file)
                    .map(|target| (file.clone(), target.to_owned(), line)),
            );
        }
    }
    for (file, target, line) in gained {
        let Some(mut node) = b.nodes.remove(&file) else {
            continue;
        };
        let test = b
            .facts
            .get(&file)
            .is_some_and(|facts| facts.test_scoped(line));
        add_edge(b, &mut node, &target, NAMESPACE_USE, line, &|_| test);
        if node
            .edges
            .get(&target)
            .is_some_and(|kinds| kinds.contains_key(NAMESPACE_USE))
            && let Some(facts) = b.facts.get_mut(&file)
        {
            facts
                .file_edges
                .push((target, NAMESPACE_USE.to_owned(), line));
        }
        b.nodes.insert(file, node);
    }
}
