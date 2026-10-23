use super::super::{ValidationIssue, union};
use super::{
    BASE_FIELD, CLONE_SUFFIX, HEAD_FIELD, NUMBER_FIELD, PATH_FIELD, Pair, REF_FIELD, field_path,
    fixed_issue,
};
use serde_json::{Map, Value, json};

/// A repository web URL's parts.
pub(super) struct WebUrl<'a> {
    text: &'a str,
    owner: String,
    repo: String,
    git_ref: Option<String>,
    file: Option<String>,
    /// The `#L3-L9` anchor of a file, as its span (`3-9`).
    lines: Option<String>,
    /// The pull request, issue, commit or comparison the URL names.
    item: Option<Item>,
    /// The ref may end inside the file path, or the URL selects more than
    /// the fields carry.
    approximate: bool,
}

/// What a pull request, issue, commit or compare URL names: its kind (the
/// URL segment) and the row fields that carry it.
struct Item {
    kind: String,
    fields: Vec<(&'static str, Value)>,
}

impl Item {
    fn parse(kind: &str, id: &str) -> Option<Self> {
        let fields = match kind {
            "pull" | "issues" => {
                let number = id.parse::<u64>().ok().filter(|number| *number > 0)?;
                vec![(NUMBER_FIELD, json!(number))]
            }
            "commit"
                if (7..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_hexdigit()) =>
            {
                vec![(REF_FIELD, json!(id))]
            }
            "compare" => {
                let (base, head) = id.split_once("...").or_else(|| id.split_once(".."))?;
                (!base.is_empty() && !head.is_empty() && !head.starts_with('.')).then_some(())?;
                vec![(BASE_FIELD, json!(base)), (HEAD_FIELD, json!(head))]
            }
            _ => return None,
        };
        Some(Self {
            kind: kind.to_owned(),
            fields,
        })
    }

    /// The row selector whose literal the URL's kind spells (`pull` →
    /// `operation:"pullRequest"`), among the properties' const and enum
    /// literals.
    fn selector(&self, properties: &Map<String, Value>) -> Option<(String, Value)> {
        properties.iter().find_map(|(name, schema)| {
            let literals = schema
                .get("const")
                .cloned()
                .into_iter()
                .chain(
                    schema
                        .get("enum")
                        .and_then(Value::as_array)
                        .cloned()
                        .into_iter()
                        .flatten(),
                )
                .collect::<Vec<_>>();
            let literal = super::super::enum_value::enum_value(&literals, &json!(self.kind), name)?;
            Some((name.clone(), json!(literal)))
        })
    }
}

impl<'a> WebUrl<'a> {
    pub(super) fn parse(text: &'a str, bare_host: bool) -> Option<Self> {
        let trimmed = text.trim();
        let url = match url::Url::parse(trimmed) {
            Ok(url) => url,
            Err(_) if bare_host => url::Url::parse(&format!("https://{trimmed}")).ok()?,
            Err(_) => return None,
        };
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let segments = url
            .path_segments()?
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let github = matches!(url.host_str(), Some(host) if host.eq_ignore_ascii_case("github.com") || host.eq_ignore_ascii_case("www.github.com"));
        if github
            && let [owner, repo, kind, id] = segments.as_slice()
            && let Some(item) = Item::parse(kind, id)
        {
            return Some(Self {
                text,
                owner: (*owner).to_owned(),
                repo: repo.strip_suffix(CLONE_SUFFIX).unwrap_or(repo).to_owned(),
                git_ref: None,
                file: None,
                lines: None,
                item: Some(item),
                approximate: url.query().is_some() || url.fragment().is_some(),
            });
        }
        // An encoded segment would need decoding to name the file exactly.
        if segments.iter().any(|segment| segment.contains('%')) {
            return None;
        }
        let host = url.host_str()?.to_ascii_lowercase();
        let (owner, repo, git_ref, file) = match (host.as_str(), segments.as_slice()) {
            ("github.com" | "www.github.com", [owner, repo]) => (*owner, *repo, None, &[][..]),
            (
                "github.com" | "www.github.com",
                [owner, repo, "blob" | "tree", git_ref, file @ ..],
            )
            | ("raw.githubusercontent.com", [owner, repo, git_ref, file @ ..])
                if !file.is_empty() || !host.starts_with("raw.") =>
            {
                (*owner, *repo, Some(*git_ref), file)
            }
            _ => return None,
        };
        let full_sha = git_ref.is_some_and(|git_ref| {
            git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit())
        });
        let lines = url
            .fragment()
            .filter(|_| !file.is_empty())
            .and_then(line_anchor);
        Some(Self {
            text,
            owner: owner.to_owned(),
            repo: repo.strip_suffix(CLONE_SUFFIX).unwrap_or(repo).to_owned(),
            git_ref: git_ref.map(str::to_owned),
            file: (!file.is_empty()).then(|| file.join("/")),
            approximate: (!file.is_empty() && !full_sha)
                || url.query().is_some()
                || (url.fragment().is_some() && lines.is_none()),
            lines,
            item: None,
        })
    }

    /// The issue at `field` (holding the URL): its fix sets the pair, ref,
    /// path and the line span (in `span`, the row's span field), or the
    /// item's selector and fields, unless the row declares no field for a
    /// part or already sets one to another value. `None` for an item this
    /// form has no selector or fields for.
    pub(super) fn issue(
        &self,
        pair: &Pair,
        properties: &Map<String, Value>,
        object: &Map<String, Value>,
        path: &[String],
        field: &str,
        span: Option<&str>,
    ) -> Option<ValidationIssue> {
        let mut fields = vec![
            (pair.first.to_owned(), Some(json!(self.owner))),
            (pair.second.to_owned(), Some(json!(self.repo))),
        ];
        let mut complete = true;
        let mut approximate = self.approximate;
        if let Some(item) = &self.item {
            // A form without the item's selector and fields does not read it.
            let (selector, literal) = item.selector(properties)?;
            item.fields
                .iter()
                .all(|(target, _)| properties.contains_key(*target))
                .then_some(())?;
            fields.push((selector, Some(literal)));
            fields.extend(
                item.fields
                    .iter()
                    .map(|(target, value)| ((*target).to_owned(), Some(value.clone()))),
            );
        }
        for (target, part) in [(REF_FIELD, &self.git_ref), (PATH_FIELD, &self.file)] {
            match part {
                Some(value) if properties.contains_key(target) => {
                    fields.push((target.to_owned(), Some(json!(value))));
                }
                Some(_) => complete = false,
                // The URL names no file: the path that held it goes.
                None if target == field => fields.push((target.to_owned(), None)),
                None => {}
            }
        }
        match (&self.lines, span) {
            (Some(lines), Some(span)) if !object.contains_key(span) => {
                fields.push((span.to_owned(), Some(json!([lines]))));
            }
            (Some(_), _) => approximate = true,
            (None, _) => {}
        }
        complete &= fields.iter().all(|(target, value)| {
            target == field
                || object
                    .get(target)
                    .is_none_or(|current| Some(current) == value.as_ref())
        });
        let shown = fields
            .iter()
            .filter_map(|(target, value)| Some(format!("{target}:{}", value.as_ref()?)))
            .collect::<Vec<_>>()
            .join(", ");
        let approximate = if approximate {
            vec![REF_FIELD.to_owned(), PATH_FIELD.to_owned()]
        } else {
            Vec::new()
        };
        Some(fixed_issue(
            "schema.repository-url",
            field_path(path, field),
            format!("\"{}\" is a repository web URL; send {shown}", self.text),
            if complete { fields } else { Vec::new() },
            approximate,
            &json!(self.text),
            union::speculative(),
        ))
    }
}

/// A file URL's line anchor (`L3`, `L3-L9`) as its span (`3-3`, `3-9`);
/// `None` for any other fragment or a descending span.
fn line_anchor(fragment: &str) -> Option<String> {
    let line = |text: &str| {
        text.strip_prefix('L')?
            .parse::<u64>()
            .ok()
            .filter(|line| *line > 0)
    };
    let (start, end) = match fragment.split_once('-') {
        Some((start, end)) => (line(start)?, line(end)?),
        None => (line(fragment)?, line(fragment)?),
    };
    (start <= end).then(|| format!("{start}-{end}"))
}
