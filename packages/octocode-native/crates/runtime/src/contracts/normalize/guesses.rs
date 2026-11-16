//! Row fields agents send for a declared one. A guess with exactly one reading
//! is read as that field, and the call says so; one with several readings is
//! left for validation, whose error names the field to use. The schema stays
//! the full input: these are repairs of what was sent, never aliases it lists.
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// The declared fields a sent one reads as, with their values: one, or a
/// field with the field that says how its value reads (`matchString` with
/// `regexEngine`).
type Fields = Vec<(&'static str, Value)>;

/// How a guess reads the value sent; what it does not read has more than
/// one reading, and validation refuses it naming the field.
pub(super) enum Read {
    /// Never: a former name of the field.
    Refused,
    /// `true` as this value of the field.
    Switch(&'static str),
    /// A text, as it is.
    Text,
    /// A scalar (text, number or switch), as it is: the field's own
    /// validation judges the value.
    Same,
    /// A text, or a list of texts, as a list.
    Listed,
    /// By the value's shape and the row it sits in.
    Shaped(fn(&Value, &Map<String, Value>) -> Option<Fields>),
}

/// A field sent for `field` of the tools in `tools`, read as `read` says.
pub(super) struct Guess {
    pub(super) tools: &'static [ToolId],
    /// The row field whose object holds `sent`: empty for the row itself.
    pub(super) within: &'static str,
    pub(super) sent: &'static str,
    pub(super) field: &'static str,
    /// What `field` takes, where the sent name promises something else: said
    /// with the field when the guess is refused.
    takes: Option<&'static str>,
    pub(super) read: Read,
}

impl Guess {
    fn reading(&self, value: &Value, row: &Map<String, Value>) -> Option<Fields> {
        let meant = match (&self.read, value) {
            (Read::Switch(meant), Value::Bool(true)) => json!(meant),
            (Read::Text, Value::String(_)) => value.clone(),
            (Read::Same, Value::String(_) | Value::Number(_) | Value::Bool(_)) => value.clone(),
            (Read::Listed, Value::String(_)) => json!([value]),
            (Read::Listed, Value::Array(items)) if items.iter().all(Value::is_string) => {
                value.clone()
            }
            (Read::Shaped(read), _) => return read(value, row),
            _ => return None,
        };
        Some(vec![(self.field, meant)])
    }
}

const SEARCH: &[ToolId] = &[ToolId::LocalSearch];
/// The tools whose walks take `maxDepth` and no `depth`.
const WALKS: &[ToolId] = &[
    ToolId::LocalSearch,
    ToolId::StructureSearch,
    ToolId::AstSearch,
    ToolId::GhStructure,
];
/// The tools whose hop limit is `depth` and no `maxDepth`.
const HOPS: &[ToolId] = &[ToolId::GraphQuery, ToolId::LspSearch];
const READERS: &[ToolId] = &[ToolId::LocalFetch, ToolId::GhGetFileContent];
/// The GitHub tools whose rows pin a commit by `ref` alone.
const GH_REFS: &[ToolId] = &[
    ToolId::GhGetFileContent,
    ToolId::GhStructure,
    ToolId::GhSearchCode,
    ToolId::GhSearchHistory,
    ToolId::GhGetHistoryItem,
    ToolId::GhCloneRepo,
];

/// `text` lower-cased without separators: `pull_request` and `PullRequest`
/// are one spelling.
fn folded(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The history search a kind of item names: pull requests (`pr`, GitHub's
/// `type:pr`), issues or commits, in the singular or plural.
fn history_operation(value: &Value, _: &Map<String, Value>) -> Option<Fields> {
    let operation = match folded(value.as_str()?).as_str() {
        "pr" | "prs" | "pull" | "pulls" | "pullrequest" | "pullrequests" => "pullRequest",
        "issue" | "issues" => "issue",
        "commit" | "commits" => "commit",
        _ => return None,
    };
    Some(vec![("operation", json!(operation))])
}

/// A guess at a row field.
const fn row(
    tools: &'static [ToolId],
    sent: &'static str,
    field: &'static str,
    read: Read,
) -> Guess {
    Guess {
        tools,
        within: "",
        sent,
        field,
        takes: None,
        read,
    }
}

/// A guess at a row field that says what `field` takes when refused.
const fn takes(
    tools: &'static [ToolId],
    sent: &'static str,
    field: &'static str,
    takes: &'static str,
    read: Read,
) -> Guess {
    Guess {
        takes: Some(takes),
        ..row(tools, sent, field, read)
    }
}

pub(super) const GUESSES: &[Guess] = &[
    takes(
        SEARCH,
        "mode",
        "regexEngine",
        "literal, rust or pcre2 (a files-only listing is view:\"files\")",
        // An engine, or the former mode that listed matching files only; a
        // regex runs in `rust` or `pcre2`: two readings.
        Read::Shaped(|value, _| match value.as_str()? {
            engine @ ("literal" | "rust" | "pcre2") => Some(vec![("regexEngine", json!(engine))]),
            "discovery" => Some(vec![("view", json!("files"))]),
            _ => None,
        }),
    ),
    row(SEARCH, "literal", "regexEngine", Read::Switch("literal")),
    row(SEARCH, "regex", "regexEngine", Read::Switch("rust")),
    // After the guesses that set `regexEngine`: an engine in the row says
    // how the value reads. `matchString` is literal text unless the row
    // names an engine, while `pattern` names a regex in most tools: terms
    // that read as a regex matching other text are that regex, run by the
    // engine the field's error names.
    takes(
        SEARCH,
        "pattern",
        "matchString",
        "literal text, or a regex with regexEngine:\"rust\"",
        Read::Shaped(|value, row| {
            let terms: Vec<&str> = match value {
                Value::String(term) => vec![term],
                Value::Array(items) => items.iter().map(Value::as_str).collect::<Option<_>>()?,
                _ => return None,
            };
            let mut fields = vec![("matchString", value.clone())];
            if !row.contains_key("regexEngine") && reads_as_regex(&terms) {
                fields.push(("regexEngine", json!("rust")));
            }
            Some(fields)
        }),
    ),
    // ripgrep's `--type`.
    takes(
        SEARCH,
        "type",
        "fileType",
        "a ripgrep file type, e.g. rust",
        Read::Text,
    ),
    row(SEARCH, "filesOnly", "view", Read::Switch("files")),
    row(SEARCH, "matchOnly", "view", Read::Switch("matchOnly")),
    // `sort` is `{by, order}`: a bare key names no order, so only a
    // `{by, order}` object is read; a key alone is refused naming `sort`.
    row(
        &[ToolId::StructureSearch],
        "sortBy",
        "sort",
        Read::Shaped(|value, _| {
            (value.get("by").is_some() && value.get("order").is_some())
                .then(|| vec![("sort", value.clone())])
        }),
    ),
    row(&[ToolId::ArtifactSearch], "name", "packageName", Read::Text),
    row(WALKS, "depth", "maxDepth", Read::Same),
    row(HOPS, "maxDepth", "depth", Read::Same),
    // `git clone --depth`: commits of history.
    row(&[ToolId::GhCloneRepo], "depth", "historyDepth", Read::Same),
    row(SEARCH, "countOnly", "view", Read::Switch("countMatches")),
    row(SEARCH, "perFileCount", "view", Read::Switch("countMatches")),
    row(SEARCH, "excludeDir", "exclude", Read::Listed),
    // `find -name`: a glob on entry names. A name without a glob is that
    // name or, as `fd` reads it, names holding it; a path is no name.
    takes(
        &[ToolId::StructureSearch],
        "name",
        "include",
        "globs on names: \"x.ts\" for that name, \"*x*\" for names holding x",
        Read::Shaped(|value, _| {
            let name = value.as_str()?;
            (name.contains(['*', '?', '[']) && !name.contains(['/', '\\']))
                .then(|| vec![("include", json!([name]))])
        }),
    ),
    // A pattern on names written in regex syntax a glob lacks (`^`, `$`,
    // `\\`, a group, `|`, `+`) is that regex, which only a files listing
    // takes; a glob or a bare name has more than one reading.
    takes(
        &[ToolId::StructureSearch],
        "pattern",
        "nameRegex",
        "a regex on entry names, with operation:\"files\"",
        Read::Shaped(|value, _| {
            let pattern = value.as_str()?;
            (pattern.contains(['^', '$', '\\', '(', ')', '|', '+'])
                && regex::Regex::new(pattern).is_ok())
            .then(|| vec![("nameRegex", json!(pattern)), ("operation", json!("files"))])
        }),
    ),
    // A GitHub search query of plain words: the terms it ANDs. A qualifier,
    // operator, quote, group or exclusion is a field or phrase of its own.
    takes(
        &[ToolId::GhSearchCode],
        "query",
        "keywords",
        "ANDed literal terms; owner, repo, path and other qualifiers are fields of their own",
        Read::Shaped(|value, _| {
            let words: Vec<&str> = value.as_str()?.split_whitespace().collect();
            let plain = |word: &&str| {
                !word.contains([':', '"', '(', ')', '\\'])
                    && !word.starts_with('-')
                    && !["AND", "OR", "NOT"]
                        .iter()
                        .any(|operator| word.eq_ignore_ascii_case(operator))
            };
            (!words.is_empty() && words.iter().all(plain)).then(|| vec![("keywords", json!(words))])
        }),
    ),
    // GitHub's own words for what a history search reads: its REST `type`
    // qualifier (`type:pr`), or the kind of item every row names.
    row(
        &[ToolId::GhSearchHistory],
        "type",
        "operation",
        Read::Shaped(history_operation),
    ),
    row(
        &[ToolId::GhSearchHistory],
        "kind",
        "operation",
        Read::Shaped(history_operation),
    ),
    // GitHub's `in:` qualifier: `in:file` searches content, `in:path` paths.
    row(
        &[ToolId::GhSearchCode],
        "match",
        "matchIn",
        Read::Shaped(|value, _| {
            let sent: Vec<&str> = match value {
                Value::String(one) => vec![one],
                Value::Array(items) => items.iter().map(Value::as_str).collect::<Option<_>>()?,
                _ => return None,
            };
            let read = sent
                .iter()
                .map(|place| match folded(place).as_str() {
                    "file" | "content" | "code" => Some("content"),
                    "path" => Some("path"),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            Some(vec![("matchIn", json!(read))])
        }),
    ),
    // Every GitHub row names the commit it read `commitSha`; the input that
    // pins it is `ref`.
    row(GH_REFS, "commitSha", "ref", Read::Text),
    // GitHub's compare API names its two sides `base` and `head`. `number`
    // alone is never read as an `operation`: an issue or a pull request.
    row(&[ToolId::GhGetHistoryItem], "base", "baseRef", Read::Text),
    row(&[ToolId::GhGetHistoryItem], "head", "headRef", Read::Text),
    // The symbol a row is about: `symbolName` of a language server query;
    // of a graph query, the node `target`, or the exact name `symbolName` of
    // the two operations that filter by one.
    row(
        &[ToolId::LspSearch],
        "symbol",
        "symbolName",
        Read::Shaped(|value, row| {
            let operation = row.get("operation").and_then(Value::as_str);
            let takes_name = !matches!(
                operation,
                Some("documentSymbols" | "diagnostic" | "workspaceSymbol")
            );
            (takes_name && value.is_string()).then(|| vec![("symbolName", value.clone())])
        }),
    ),
    row(
        &[ToolId::GraphQuery],
        "symbol",
        "target",
        Read::Shaped(|value, row| {
            let field = match row.get("operation").and_then(Value::as_str)? {
                "witnesses" | "unresolved" => "symbolName",
                _ => "target",
            };
            value.is_string().then(|| vec![(field, value.clone())])
        }),
    ),
    // ripgrep's `-c`: matches counted per file.
    row(SEARCH, "count", "view", Read::Switch("countMatches")),
    // ast-grep's `--lang` / `language:`.
    row(&[ToolId::AstSearch], "language", "parser", Read::Text),
    // `startLine` and `endLine` beside `path` read as one range below.
    row(READERS, "startLine", "ranges", Read::Refused),
    row(READERS, "endLine", "ranges", Read::Refused),
    // Former names of renamed fields.
    row(
        &[ToolId::StructureSearch],
        "sizeBytes",
        "bytes",
        Read::Refused,
    ),
    row(
        &[ToolId::LspSearch],
        "symbolQuery",
        "symbolFuzzy",
        Read::Refused,
    ),
    row(&[ToolId::GraphQuery], "name", "symbolName", Read::Refused),
    // Filter names follow the output fields they filter.
    Guess {
        within: "filters",
        ..row(&[ToolId::GhSearchRepo], "forkCount", "forks", Read::Refused)
    },
    Guess {
        within: "filters",
        ..row(
            &[ToolId::GhSearchHistory],
            "commentCount",
            "commentsCount",
            Read::Refused,
        )
    },
    Guess {
        within: "filters",
        ..row(&[ToolId::GhSearchHistory], "merged", "isMerged", Read::Same)
    },
];

/// A value a declared field refuses that has one reading: as another value
/// of it, or as a value of another field of the same object. A value in
/// another case or separator style of one the field takes needs no entry:
/// validation reads it (`normalize::repaired`).
pub(super) struct Spelling {
    pub(super) tools: &'static [ToolId],
    /// The row field whose object holds `field`: empty for the row itself.
    pub(super) within: &'static str,
    pub(super) field: &'static str,
    /// The value sent, in any case or separator style.
    pub(super) sent: &'static str,
    /// The field it reads as, and that field's value as JSON text.
    pub(super) reads: (&'static str, &'static str),
}

pub(super) const SPELLINGS: &[Spelling] = &[
    // GitHub's `is:merged`: a merged pull request is closed and merged.
    Spelling {
        tools: &[ToolId::GhSearchHistory],
        within: "filters",
        field: "state",
        sent: "merged",
        reads: ("isMerged", "true"),
    },
    // The registry's own name and its host name: each tool takes one.
    Spelling {
        tools: &[ToolId::SecurityCheck],
        within: "",
        field: "ecosystem",
        sent: "crates",
        reads: ("ecosystem", "\"crates.io\""),
    },
    Spelling {
        tools: &[ToolId::ArtifactSearch],
        within: "",
        field: "ecosystem",
        sent: "crates.io",
        reads: ("ecosystem", "\"crates\""),
    },
    // The operation each tool's callers know under another name.
    Spelling {
        tools: &[ToolId::GhSearchHistory],
        within: "",
        field: "operation",
        sent: "commits",
        reads: ("operation", "\"commit\""),
    },
    Spelling {
        tools: &[ToolId::GhSearchHistory],
        within: "",
        field: "operation",
        sent: "pullRequests",
        reads: ("operation", "\"pullRequest\""),
    },
    Spelling {
        tools: &[ToolId::GhSearchHistory],
        within: "",
        field: "operation",
        sent: "issues",
        reads: ("operation", "\"issue\""),
    },
    Spelling {
        tools: &[ToolId::LspSearch],
        within: "",
        field: "operation",
        sent: "outline",
        reads: ("operation", "\"documentSymbols\""),
    },
    Spelling {
        tools: &[ToolId::StructureSearch],
        within: "",
        field: "operation",
        sent: "find",
        reads: ("operation", "\"files\""),
    },
    Spelling {
        tools: &[ToolId::ArtifactSearch],
        within: "",
        field: "ecosystem",
        sent: "cargo",
        reads: ("ecosystem", "\"crates\""),
    },
    // A size sort has one key: the file's size in bytes, spelled `bytes` (as
    // the `bytes` filter is) or, in a contract that still names it so,
    // `sizeBytes`; a reading applies only where the contract declares its
    // value ([`Spelling::declared`]).
    Spelling {
        tools: &[ToolId::GhStructure, ToolId::StructureSearch],
        within: "sort",
        field: "by",
        sent: "size",
        reads: ("by", "\"sizeBytes\""),
    },
    Spelling {
        tools: &[ToolId::GhStructure, ToolId::StructureSearch],
        within: "sort",
        field: "by",
        sent: "size",
        reads: ("by", "\"bytes\""),
    },
    Spelling {
        tools: &[ToolId::GhStructure, ToolId::StructureSearch],
        within: "sort",
        field: "by",
        sent: "sizeBytes",
        reads: ("by", "\"bytes\""),
    },
];

/// A value a list field of one operation refuses that names what every read
/// of that operation already returns under another field. It adds nothing:
/// it is left out (the list too when it held nothing else), and the note
/// names the field that carries it.
pub(super) struct Returned {
    pub(super) tools: &'static [ToolId],
    pub(super) operation: &'static str,
    pub(super) field: &'static str,
    /// The item sent, in any case or separator style.
    pub(super) sent: &'static str,
    /// The output field every read of `operation` carries it in.
    pub(super) returned_as: &'static str,
}

pub(super) const RETURNED: &[Returned] = &[
    // A commit's body is its message past the first line; every commit read
    // returns the whole `message`.
    Returned {
        tools: &[ToolId::GhGetHistoryItem],
        operation: "commit",
        field: "sections",
        sent: "body",
        returned_as: "message",
    },
];

impl Spelling {
    /// The field and value `value` of `field` reads as, when it is this
    /// spelling's.
    fn reading(&self, field: &str, value: &Value) -> Option<(&'static str, Value)> {
        let sent = value.as_str()?;
        if field != self.field || !crate::contracts::validate::same_name(sent, self.sent) {
            return None;
        }
        let meant = serde_json::from_str(self.reads.1).ok()?;
        Some((self.reads.0, meant))
    }

    /// Whether `tool`'s contract takes the value this spelling reads: a
    /// reading of a value the field does not declare would trade one refused
    /// value for another. A reading into another field always applies.
    pub(super) fn declared(&self, id: ToolId) -> bool {
        use super::super::schema_facts::{child_schemas, resolve_ref};
        let (field, meant) = self.reads;
        if field != self.field {
            return true;
        }
        let Ok(tool) = super::super::tool_contract(id) else {
            return false;
        };
        let (query, defs) = (&tool["querySchema"], &tool["querySchema"]["$defs"]);
        let holders = match self.within {
            "" => vec![query],
            within => child_schemas(query, defs, within),
        };
        let Ok(meant) = serde_json::from_str::<Value>(meant) else {
            return false;
        };
        holders
            .iter()
            .flat_map(|holder| child_schemas(holder, defs, field))
            .any(|schema| lists(resolve_ref(schema, defs), defs, &meant))
    }
}

/// Whether `schema`, or a branch of it, lists `value` among its literals.
fn lists(schema: &Value, defs: &Value, value: &Value) -> bool {
    let schema = super::super::schema_facts::resolve_ref(schema, defs);
    schema["enum"]
        .as_array()
        .is_some_and(|values| values.contains(value))
        || schema.get("const") == Some(value)
        || ["anyOf", "oneOf"].iter().any(|keyword| {
            schema[*keyword]
                .as_array()
                .is_some_and(|branches| branches.iter().any(|branch| lists(branch, defs, value)))
        })
}

/// `field: value` sent in the object `within` of an `id` row, read as the
/// one spelling it is: the field and value it means, else as sent.
fn spelled(id: ToolId, within: &str, field: &str, value: Value) -> (String, Value) {
    let read = SPELLINGS
        .iter()
        .filter(|spelling| spelling.within == within && spelling.tools.contains(&id))
        .filter(|spelling| spelling.declared(id))
        .find_map(|spelling| spelling.reading(field, &value));
    match read {
        Some((field, meant)) => (field.to_owned(), meant),
        None => (field.to_owned(), value),
    }
}

/// Whether literal `terms` also read as a regex matching other text: some
/// term's regex reading is not its own spelling and every term compiles.
/// Text whose regex reading spells itself (`defects-queue`: `-` is literal
/// outside a class), or that does not compile (`call(`), has the literal
/// reading alone.
fn reads_as_regex(terms: &[&str]) -> bool {
    use octocode_engine::search::text_pattern::regex_spells_itself;
    terms.iter().any(|term| !regex_spells_itself(term))
        && terms.iter().all(|term| regex::Regex::new(term).is_ok())
}

/// The declared field a sent `field` of `tool` stands for, when it is a known
/// guess, and what that field takes when the sent name promises something
/// else: what an unknown-field error tells the caller to use. `within` names
/// the row field whose object holds `field`, empty for the row itself.
pub(crate) fn meant_field(
    tool: &str,
    within: &str,
    field: &str,
) -> Option<(&'static str, Option<&'static str>)> {
    let id = ToolId::from_name(tool)?;
    GUESSES
        .iter()
        .find(|guess| guess.within == within && guess.sent == field && guess.tools.contains(&id))
        .map(|guess| (guess.field, guess.takes))
}

/// A repair the guesses made on a row, said once the call's values are final.
pub(super) enum Note {
    /// Said as made.
    Said(String),
    /// `sent` read as `fields`, each with the value the guess gave it: said
    /// with the value each field runs with where that is not the value sent,
    /// so a value validation then reads in its field's shape (`pattern:"x"`
    /// runs as `matchString:["x"]`) is said once, with the field.
    Read {
        sent: &'static str,
        value: Value,
        fields: Vec<(&'static str, Value)>,
    },
}

impl Note {
    /// Whether the note names `field` of its row, as it runs.
    pub(super) fn writes(&self, field: &str) -> bool {
        match self {
            Self::Said(_) => false,
            Self::Read { fields, .. } => fields.iter().any(|(written, _)| *written == field),
        }
    }

    /// The note's text, given the row as the call runs it.
    pub(super) fn said(self, ran: &Value) -> String {
        let (sent, value, fields) = match self {
            Self::Said(note) => return note,
            Self::Read {
                sent,
                value,
                fields,
            } => (sent, value, fields),
        };
        // The repair names each value the call runs with other than the
        // one sent (`filesOnly: true` is `view: "files"`).
        let repair = fields
            .iter()
            .map(|(field, merged)| {
                let runs = ran.get(*field).unwrap_or(merged);
                if *runs == value {
                    format!("`{field}`")
                } else {
                    format!("`{field}: {}`", super::sketch(runs))
                }
            })
            .collect::<Vec<_>>()
            .join(" with ");
        format!("read `{sent}` as {repair}; send {repair} next time.")
    }
}

/// Read each row's guesses that have one reading as their declared fields;
/// what was read, one note per repair and row it was made on.
pub(super) fn read(id: ToolId, input: &mut Value) -> Vec<(usize, Note)> {
    let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        let Some(row) = row.as_object_mut() else {
            continue;
        };
        let mut said = |note: String| notes.push((index, Note::Said(note)));
        if READERS.contains(&id) {
            read_range(row).into_iter().for_each(&mut said);
            read_range_items(row).into_iter().for_each(&mut said);
        }
        read_tool_queries(id, row)
            .into_iter()
            .chain(read_switches(id, row))
            .chain(drop_retired_cursors(id, row))
            .chain(read_source_query(id, row))
            .for_each(&mut said);
        for guess in GUESSES
            .iter()
            .filter(|guess| guess.within.is_empty() && guess.tools.contains(&id))
        {
            let Some(value) = row.get(guess.sent) else {
                continue;
            };
            let Some(fields) = guess.reading(value, row) else {
                continue;
            };
            let Some(read) = fields
                .into_iter()
                .map(|(field, meant)| Some((field, merged(row, field, meant)?)))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let value = row.remove(guess.sent).unwrap_or_default();
            for (field, merged) in &read {
                row.insert((*field).to_owned(), merged.clone());
            }
            notes.push((
                index,
                Note::Read {
                    sent: guess.sent,
                    value,
                    fields: read,
                },
            ));
        }
        let mut said = |note: String| notes.push((index, Note::Said(note)));
        read_owned(id, row).into_iter().for_each(&mut said);
        read_within(id, row).into_iter().for_each(&mut said);
        read_spellings(id, row).into_iter().for_each(&mut said);
    }
    notes
}

/// Each row field no form of the row declares that exactly one row field's
/// object declares (`state` of `filters`, `pattern` of `rule`), read inside
/// that object, as the [`SPELLINGS`] there read it. A field the object
/// already sets, a value beside an owner that is no object, or a field
/// several objects declare has no one reading. One note per field read.
fn read_owned(id: ToolId, row: &mut Map<String, Value>) -> Vec<String> {
    use super::super::schema_facts::{property_names, property_schemas};
    let Ok(tool) = super::super::tool_contract(id) else {
        return Vec::new();
    };
    let (query, defs) = (&tool["querySchema"], &tool["querySchema"]["$defs"]);
    let declared = property_names(query, defs);
    let guessed = |key: &str| {
        GUESSES
            .iter()
            .any(|guess| guess.within.is_empty() && guess.sent == key && guess.tools.contains(&id))
    };
    let sent = row
        .keys()
        .filter(|key| !declared.contains(key.as_str()) && !guessed(key))
        .cloned()
        .collect::<Vec<_>>();
    let mut notes = Vec::new();
    for key in sent {
        let owners = declared
            .iter()
            .copied()
            .filter(|owner| {
                property_schemas(query, defs, owner)
                    .iter()
                    .any(|schema| property_names(schema, defs).contains(key.as_str()))
            })
            .collect::<Vec<&str>>();
        let [owner] = owners.as_slice() else {
            continue;
        };
        let owner: &str = owner;
        let (field, value) = spelled(id, owner, &key, row[&key].clone());
        let holder = row.get(owner).cloned().unwrap_or_else(|| json!({}));
        let Value::Object(mut holder) = holder else {
            continue;
        };
        if holder.contains_key(&field) {
            continue;
        }
        holder.insert(field, value);
        let holder = Value::Object(holder);
        let repair = format!("`{owner}: {holder}`");
        row.remove(&key);
        row.insert(owner.to_owned(), holder);
        notes.push(format!(
            "read `{key}` as {repair}; send {repair} next time."
        ));
    }
    notes
}

/// The [`GUESSES`] at fields of a row field's object (`filters.merged`)
/// with one reading, read as their declared fields. One note per field read.
fn read_within(id: ToolId, row: &mut Map<String, Value>) -> Vec<String> {
    let mut notes = Vec::new();
    for guess in GUESSES
        .iter()
        .filter(|guess| !guess.within.is_empty() && guess.tools.contains(&id))
    {
        let Some(Value::Object(holder)) = row.get_mut(guess.within) else {
            continue;
        };
        let Some(value) = holder.get(guess.sent) else {
            continue;
        };
        let Some(fields) = guess.reading(value, holder) else {
            continue;
        };
        let Some(read) = fields
            .into_iter()
            .map(|(field, meant)| Some((field, merged(holder, field, meant)?)))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        holder.remove(guess.sent);
        let within = guess.within;
        let repair = read
            .iter()
            .map(|(field, merged)| format!("`{within}.{field}: {merged}`"))
            .collect::<Vec<_>>()
            .join(" with ");
        for (field, merged) in read {
            holder.insert(field.to_owned(), merged);
        }
        notes.push(format!(
            "read `{within}.{}` as {repair}; send {repair} next time.",
            guess.sent
        ));
    }
    notes
}

/// Each [`SPELLINGS`] value of the row, or of a row field's object, read as
/// the field and value it means. One note per value read.
fn read_spellings(id: ToolId, row: &mut Map<String, Value>) -> Vec<String> {
    let mut notes = Vec::new();
    for spelling in SPELLINGS
        .iter()
        .filter(|spelling| spelling.tools.contains(&id) && spelling.declared(id))
    {
        let holder = match spelling.within {
            "" => Some(&mut *row),
            within => row.get_mut(within).and_then(Value::as_object_mut),
        };
        let Some(holder) = holder else {
            continue;
        };
        let Some(sent) = holder.get(spelling.field).cloned() else {
            continue;
        };
        let Some((field, meant)) = spelling.reading(spelling.field, &sent) else {
            continue;
        };
        if field != spelling.field && holder.get(field).is_some_and(|have| *have != meant) {
            continue;
        }
        holder.remove(spelling.field);
        holder.insert(field.to_owned(), meant.clone());
        let at = |field: &str| match spelling.within {
            "" => field.to_owned(),
            within => format!("{within}.{field}"),
        };
        let (from, to) = (at(spelling.field), at(field));
        notes.push(format!(
            "read `{from}: {sent}` as `{to}: {meant}`; send `{to}` next time."
        ));
    }
    notes
}

/// Each distinct repair note once, in the order first made, prefixed with
/// the rows it was made on: a batch that sent one guess on every row reads
/// one line, not one per row. A note of no row (`None`: a request field)
/// reads as made.
pub(super) fn per_call(notes: Vec<(Option<usize>, String)>) -> Vec<String> {
    let mut grouped: Vec<(Option<Vec<usize>>, String)> = Vec::new();
    for (index, note) in notes {
        let same = |(rows, said): &&mut (Option<Vec<usize>>, String)| {
            *said == note && rows.is_some() == index.is_some()
        };
        match (grouped.iter_mut().find(same), index) {
            (Some((Some(rows), _)), Some(index)) if !rows.contains(&index) => rows.push(index),
            (Some(_), _) => {}
            (None, index) => grouped.push((index.map(|index| vec![index]), note)),
        }
    }
    // In row order, a request field's notes last; a row's in the order made.
    grouped.sort_by_key(|(rows, _)| rows.as_ref().map_or(usize::MAX, |rows| rows[0]));
    grouped
        .into_iter()
        .map(|(rows, note)| {
            let Some(rows) = rows else {
                return note;
            };
            let rows = rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("queries[{rows}]: {note}")
        })
        .collect()
}

/// `meant` for `field` of `row`, beside what the row already sets there;
/// none when the row sets another value: two values, no one reading.
fn merged(row: &Map<String, Value>, field: &str, meant: Value) -> Option<Value> {
    match (row.get(field), meant) {
        (None, meant) => Some(meant),
        // Paths to skip add up: both lists are one reading.
        (Some(Value::Array(have)), Value::Array(more)) if field == "exclude" => {
            Some(Value::Array(have.iter().chain(&more).cloned().collect()))
        }
        (Some(have), meant) => (*have == meant).then_some(meant),
    }
}

/// A switch (`true`, `false`) sent for a field whose values include `on`
/// and `off`: that value. One note per field read.
fn read_switches(id: ToolId, row: &mut Map<String, Value>) -> Vec<String> {
    use super::super::schema_facts::{child_schemas, resolve_ref};
    let Ok(tool) = super::super::tool_contract(id) else {
        return Vec::new();
    };
    let (query, defs) = (&tool["querySchema"], &tool["querySchema"]["$defs"]);
    let mut notes = Vec::new();
    for (key, value) in row.iter_mut() {
        let Some(on) = value.as_bool() else {
            continue;
        };
        let switched = child_schemas(query, defs, key).iter().any(|field| {
            resolve_ref(field, defs)["enum"]
                .as_array()
                .is_some_and(|values| {
                    values.contains(&json!("on")) && values.contains(&json!("off"))
                })
        });
        if switched {
            *value = json!(if on { "on" } else { "off" });
            notes.push(format!(
                "read `{key}: {on}` as `{key}: {value}`; send `{key}` next time."
            ));
        }
    }
    notes
}

/// Cursor fields a tool no longer takes: left out of the row, and the note
/// names the cursor that pages the result. `localSearch` once paged the hits
/// of its shown files by `matchPage`; its one cursor now pages them all.
fn drop_retired_cursors(id: ToolId, row: &mut Map<String, Value>) -> Option<String> {
    (id == ToolId::LocalSearch && row.remove("matchPage").is_some()).then(|| {
        "left out `matchPage`: localSearch pages its hits with the cursor in `next.nextPage`, sent alone as {\"cursor\": ...}.".to_owned()
    })
}

/// Fields of a clasify `source` beside its `tool`, which the tool's query
/// holds: read as that `query`. `source` declares `tool`, `query` and `url`
/// alone, so every other field names the tool's own query.
fn read_source_query(id: ToolId, row: &mut Map<String, Value>) -> Option<String> {
    if id != ToolId::Clasify {
        return None;
    }
    let source = row.get_mut("source")?.as_object_mut()?;
    if !source.get("tool").is_some_and(Value::is_string)
        || source.contains_key("query")
        || source.contains_key("url")
    {
        return None;
    }
    let sent: Vec<String> = source
        .keys()
        .filter(|key| !matches!(key.as_str(), "tool"))
        .cloned()
        .collect();
    if sent.is_empty() {
        return None;
    }
    let query: Map<String, Value> = sent
        .iter()
        .filter_map(|key| source.remove(key).map(|value| (key.clone(), value)))
        .collect();
    source.insert("query".to_owned(), Value::Object(query));
    let fields = sent
        .iter()
        .map(|key| format!("`source.{key}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "read {fields} as {}; send {} next time.",
        sent.iter()
            .map(|key| format!("`source.query.{key}`"))
            .collect::<Vec<_>>()
            .join(", "),
        "them under `source.query`",
    ))
}

/// Each `{tool, query}` field of the row sent as its query alone
/// (`source:{path}`), read as `{tool, query}` of the one tool whose closed
/// query form takes it: every sent field declared, none it requires missing.
/// A form naming several tools (a search query, checked against the named
/// tool's own schema) names no one tool, and a value sending a field of the
/// union itself (`tool`, `query`, `url`) has chosen its form: both are left
/// for validation. One note per field read.
fn read_tool_queries(id: ToolId, row: &mut Map<String, Value>) -> Vec<String> {
    let Ok(tool) = super::super::tool_contract(id) else {
        return Vec::new();
    };
    let schema = &tool["querySchema"];
    let mut notes = Vec::new();
    wrap_tool_queries(row, &[schema], &schema["$defs"], "", &mut notes);
    notes
}

/// [`read_tool_queries`] over the fields of `object`, whose schemas are
/// `schemas`, and the objects and lists nested in them.
fn wrap_tool_queries(
    object: &mut Map<String, Value>,
    schemas: &[&Value],
    defs: &Value,
    at: &str,
    notes: &mut Vec<String>,
) {
    use super::super::schema_facts::{child_schemas, item_schemas};
    for (key, value) in object.iter_mut() {
        let children = schemas
            .iter()
            .flat_map(|schema| child_schemas(schema, defs, key))
            .collect::<Vec<_>>();
        if children.is_empty() {
            continue;
        }
        let label = if at.is_empty() {
            key.clone()
        } else {
            format!("{at}.{key}")
        };
        if let Some(note) = wrap_tool_query(value, &children, defs, &label) {
            notes.push(note);
            continue;
        }
        match value {
            Value::Object(inner) => wrap_tool_queries(inner, &children, defs, &label, notes),
            Value::Array(items) => {
                let item_schemas = children
                    .iter()
                    .flat_map(|schema| item_schemas(schema, defs))
                    .collect::<Vec<_>>();
                if item_schemas.is_empty() {
                    continue;
                }
                for (index, item) in items.iter_mut().enumerate() {
                    let label = format!("{label}[{index}]");
                    if let Some(note) = wrap_tool_query(item, &item_schemas, defs, &label) {
                        notes.push(note);
                    } else if let Value::Object(inner) = item {
                        wrap_tool_queries(inner, &item_schemas, defs, &label, notes);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `value` as `{tool, query}` when its fields are the query of exactly one
/// tool among the forms of `schemas`; the note that says so.
fn wrap_tool_query(
    value: &mut Value,
    schemas: &[&Value],
    defs: &Value,
    label: &str,
) -> Option<String> {
    let sent = value.as_object().filter(|object| !object.is_empty())?;
    let mut forms = Vec::new();
    for schema in schemas {
        object_forms(schema, defs, &mut forms);
    }
    let mut tools: Vec<&str> = Vec::new();
    for form in forms {
        let Some(properties) = form.get("properties").and_then(Value::as_object) else {
            continue;
        };
        if sent.keys().any(|name| properties.contains_key(name)) {
            return None;
        }
        let (Some(tool), Some(query)) = (properties.get("tool"), properties.get("query")) else {
            continue;
        };
        let Some(tool) = single_name(super::super::schema_facts::resolve_ref(tool, defs)) else {
            continue;
        };
        let query = super::super::schema_facts::resolve_ref(query, defs);
        let declared = query.get("properties").and_then(Value::as_object);
        let takes = query.get("additionalProperties") == Some(&Value::Bool(false))
            && sent
                .keys()
                .all(|name| declared.is_some_and(|declared| declared.contains_key(name)))
            && query
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .all(|name| sent.contains_key(name));
        if takes && !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    let [tool] = tools.as_slice() else {
        return None;
    };
    let fields = sent.keys().cloned().collect::<Vec<_>>().join(",");
    let query = value.take();
    *value = json!({"tool": tool, "query": query});
    Some(format!(
        "read `{label}:{{{fields}}}` as `{label}:{{tool:\"{tool}\",query:{{{fields}}}}}`; send that next time."
    ))
}

/// The object forms `schema` stands for, through references and unions.
fn object_forms<'a>(schema: &'a Value, defs: &'a Value, forms: &mut Vec<&'a Value>) {
    let schema = super::super::schema_facts::resolve_ref(schema, defs);
    let nested = ["anyOf", "oneOf"]
        .iter()
        .find_map(|keyword| schema.get(*keyword).and_then(Value::as_array));
    match nested {
        Some(branches) => {
            for branch in branches {
                object_forms(branch, defs, forms);
            }
        }
        None => forms.push(schema),
    }
}

/// The one string a `const`, or a one-value `enum`, pins a field to.
fn single_name(schema: &Value) -> Option<&str> {
    schema.get("const").and_then(Value::as_str).or_else(|| {
        match schema.get("enum").and_then(Value::as_array)?.as_slice() {
            [only] => only.as_str(),
            _ => None,
        }
    })
}

/// `startLine` with `endLine` beside a reader's path: the one range they
/// bound. Either alone, or beside `ranges`, has no one reading.
fn read_range(row: &mut Map<String, Value>) -> Option<String> {
    if row.contains_key("ranges") {
        return None;
    }
    let (Some(start), Some(end)) = (
        row.get("startLine").filter(|line| line.is_u64()).cloned(),
        row.get("endLine").filter(|line| line.is_u64()).cloned(),
    ) else {
        return None;
    };
    row.remove("startLine");
    row.remove("endLine");
    row.insert(
        "ranges".to_owned(),
        json!([{"startLine": start, "endLine": end}]),
    );
    Some(
        "read `startLine`/`endLine` as `ranges:[{startLine,endLine}]`; send `ranges` next time."
            .to_owned(),
    )
}

/// Items of `ranges` sent as `[start, end]` pairs or `{start, end}`: the
/// ranges they bound, beside declared items. A flat list of numbers (`[2, 4]`:
/// lines 2 and 4, or 2 through 4), half a range or a bound sent twice has no
/// one reading.
fn read_range_items(row: &mut Map<String, Value>) -> Option<String> {
    let bounds = |item: &Value| match item {
        Value::Array(pair) => match pair.as_slice() {
            [start, end] => Some((start.clone(), end.clone(), true)),
            _ => None,
        },
        Value::Object(sent) if sent.len() == 2 => {
            let line = |name: &str| sent.get(name).cloned();
            match (
                line("start"),
                line("end"),
                line("startLine"),
                line("endLine"),
            ) {
                (Some(start), Some(end), ..) => Some((start, end, true)),
                (.., Some(start), Some(end)) => Some((start, end, false)),
                _ => None,
            }
        }
        _ => None,
    };
    let read = row
        .get("ranges")?
        .as_array()?
        .iter()
        .map(bounds)
        .collect::<Option<Vec<_>>>()?;
    let lines = read
        .iter()
        .all(|(start, end, _)| start.is_u64() && end.is_u64());
    if !lines || !read.iter().any(|(.., guessed)| *guessed) {
        return None;
    }
    let ranges = read
        .into_iter()
        .map(|(start, end, _)| json!({"startLine": start, "endLine": end}))
        .collect();
    row.insert("ranges".to_owned(), Value::Array(ranges));
    Some(
        "read each `[start, end]` or `{start, end}` in `ranges` as `{startLine, endLine}`; send `ranges:[{startLine,endLine}]` next time."
            .to_owned(),
    )
}
