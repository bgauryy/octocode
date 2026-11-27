//! What a tool's output contract lets the response state once: its hoist
//! plan, core's `hoistPlan` (`holder`: `{collection, fields, after?}`;
//! `omitWhen`: `{at, const | equals | template | countOf}`; `outline`: the
//! declaration lists stated as outline strings; `batch`: `{identity,
//! disclosure, hints, related}`), with the keyed batch groups of
//! `pageContextFields.batchStatedOnce`. Each entry names row pointers
//! (`/data/…`, `*` for every item or record member), read relative to a
//! result row's `data`; an omitted value applies again wherever the
//! contract's `collectionRefs` repeat its items' shape. A plan without an
//! entry states nothing else once.
use super::pattern::Pattern;
use crate::id::ToolId;
use serde_json::Value;

impl Plan {
    /// Whether any entry names a call channel (`next`, `related`) as a
    /// token or a field: then a statement could read a row's calls, and rows
    /// are matched with their calls ([`super::apply`]).
    pub(crate) fn names_calls(&self) -> bool {
        let names = |pattern: &Pattern| {
            pattern
                .tokens()
                .iter()
                .any(|token| super::pattern::CALL_CHANNELS.contains(&token.as_str()))
        };
        let fields = |fields: &[String]| {
            fields
                .iter()
                .any(|field| super::pattern::CALL_CHANNELS.contains(&field.as_str()))
        };
        self.omit.iter().any(|entry| {
            names(&entry.at)
                || match &entry.when {
                    When::Const(_) => false,
                    When::Equals(source) | When::CountOf(source) => names(source),
                    When::Template(pieces) => pieces.iter().any(|piece| match piece {
                        Piece::Text(_) => false,
                        Piece::Field(source) => names(source),
                    }),
                }
        }) || self
            .holders
            .iter()
            .any(|holder| names(&holder.at) || fields(&holder.fields) || fields(&holder.after))
            || self
                .refs
                .iter()
                .any(|reference| names(&reference.target) || names(&reference.tail))
            || self.outline.iter().any(names)
    }
}

/// One tool's hoist plan.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// Values omitted where the contract derives them (F3).
    pub(crate) omit: Vec<Omit>,
    /// Fields every item of a holder's collections shares, stated on the
    /// holder (F1), in the contract's order.
    pub(crate) holders: Vec<Holder>,
    /// What a batch states once (F4).
    pub(crate) batch: Batch,
    /// Where a collection's items repeat another's shape (a declaration's
    /// `members`), so every omitted value there applies again, at any depth.
    pub(crate) refs: Vec<Recurse>,
    /// Declaration lists stated as line-first outline strings (Q5), formed
    /// from the sealed objects.
    pub(crate) outline: Vec<Pattern>,
}

/// Items at `target` hold, at `tail` below them, items of their own shape
/// (core `collectionRefs`: `{path: target + tail, target}`).
#[derive(Debug)]
pub(crate) struct Recurse {
    pub(crate) target: Pattern,
    pub(crate) tail: Pattern,
}

/// A value the reader recovers without its field (F3).
#[derive(Debug)]
pub(crate) struct Omit {
    /// The field, under `data`.
    pub(crate) at: Pattern,
    pub(crate) when: When,
}

/// When an omitted field's value is recoverable.
#[derive(Debug)]
pub(crate) enum When {
    /// It is the schema's constant.
    Const(Value),
    /// It equals the field this pattern names, its `*`s bound to the
    /// omitted field's leading items.
    Equals(Pattern),
    /// It is this text with each `{pattern}` hole filled by that field's
    /// string, bound as [`When::Equals`].
    Template(Vec<Piece>),
    /// It is the length of the array this pattern names, bound as
    /// [`When::Equals`].
    CountOf(Pattern),
}

/// A piece of a [`When::Template`].
#[derive(Debug)]
pub(crate) enum Piece {
    Text(String),
    Field(Pattern),
}

/// Fields the items of a holder's collections may leave to the holder.
#[derive(Debug)]
pub(crate) struct Holder {
    /// The holder, under `data` (the root: `data` itself).
    pub(crate) at: Pattern,
    pub(crate) fields: Vec<String>,
    /// Fields of an earlier group no item may still carry before this group
    /// moves (a commit names a state of a repository: `commitSha` waits for
    /// `owner` and `repo`).
    pub(crate) after: Vec<String>,
}

/// What a batch of result rows states once.
#[derive(Debug, Default)]
pub(crate) struct Batch {
    /// Facts a later result whose `by` an earlier one shares omits.
    pub(crate) keyed: Vec<Keyed>,
    /// Row fields every result row states alike, stated once in the
    /// envelope's `shared.data`: identity scalars and disclosure objects,
    /// never a verdict or evidence.
    pub(crate) shared: Vec<Pattern>,
    /// Hints every empty or failed row states alike go to the envelope.
    pub(crate) hints: bool,
    /// Single-query `related` calls of one tool, one per row, merge into one
    /// envelope call.
    pub(crate) related: bool,
}

/// A group of result facts a batch states once per `by` value.
#[derive(Debug)]
pub(crate) struct Keyed {
    /// Pointer under a result row naming what the result read.
    pub(crate) by: String,
    /// Pointers under a result row of that reading's facts.
    pub(crate) fields: Vec<String>,
}

#[expect(
    clippy::expect_used,
    reason = "a missing or malformed embedded plan is a contract defect caught for every tool by contract tests"
)]
pub(crate) fn of(tool: ToolId) -> &'static Plan {
    static PLANS: [std::sync::OnceLock<Plan>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    PLANS[tool.index()].get_or_init(|| {
        let contract =
            crate::contracts::tool_contract(tool).expect("embedded contract contains every tool");
        read(contract)
    })
}

#[expect(
    clippy::expect_used,
    reason = "a malformed embedded group list is a contract defect caught for every tool by contract tests"
)]
fn read(contract: &Value) -> Plan {
    let context = &contract["pageContextFields"];
    let names = |value: &Value| {
        value
            .as_array()
            .map(|names| {
                names
                    .iter()
                    .map(|name| name.as_str().expect("group field name").to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    // Each entry names one collection; its holder is the collection's
    // parent, and a group the plan lists for several collections of one
    // holder is one group there (the holder step weighs every collection
    // that carries it).
    let mut holders: Vec<Holder> = Vec::new();
    for entry in contract["hoistPlan"]["holder"]
        .as_array()
        .expect("core declares each tool's hoist plan")
    {
        let holder = Holder {
            at: Pattern::parse(under_data(&entry["collection"]))
                .parent()
                .expect("a holder collection lies under data"),
            fields: names(&entry["fields"]),
            after: names(&entry["after"]),
        };
        if !holders.iter().any(|known| {
            known.at == holder.at && known.fields == holder.fields && known.after == holder.after
        }) {
            holders.push(holder);
        }
    }
    // A contract that declares no keyed group states every result whole.
    let keyed = context["batchStatedOnce"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|group| Keyed {
            by: group["by"].as_str().expect("group by").to_owned(),
            fields: names(&group["fields"]),
        })
        .collect();
    let plan = &contract["hoistPlan"];
    let refs = contract["collectionRefs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reference| {
            let target = Pattern::parse(under_data(&reference["target"]));
            let tail = Pattern::parse(under_data(&reference["path"])).below(&target)?;
            Some(Recurse { target, tail })
        })
        .collect();
    let outline = plan["outline"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|pointer| Pattern::parse(under_data(pointer)))
        .collect();
    Plan {
        refs,
        outline,
        omit: plan["omitWhen"]
            .as_array()
            .into_iter()
            .flatten()
            .map(omit_entry)
            .collect(),
        holders,
        batch: Batch {
            keyed,
            shared: ["identity", "disclosure"]
                .iter()
                .flat_map(|class| plan["batch"][class].as_array().into_iter().flatten())
                .map(|pointer| Pattern::parse(under_data(pointer)))
                .collect(),
            hints: plan["batch"]["hints"] == true,
            related: plan["batch"]["related"] == true,
        },
    }
}

/// A row pointer (`/data/…`) as a pattern under `data`.
#[expect(
    clippy::expect_used,
    reason = "a plan pointer outside a row's data is a contract defect caught for every tool by contract tests"
)]
fn under_data(pointer: &Value) -> &str {
    pointer
        .as_str()
        .and_then(|pointer| pointer.strip_prefix("/data"))
        .expect("a plan pointer names a row's data")
}

/// One `omitWhen` entry: `{at, const}`, `{at, equals}` or `{at, template}`,
/// a template's holes written `{<row pointer>}`.
#[expect(
    clippy::expect_used,
    reason = "a malformed omitWhen entry is a contract defect caught for every tool by contract tests"
)]
fn omit_entry(entry: &Value) -> Omit {
    let at = Pattern::parse(under_data(&entry["at"]));
    let when = if let Some(value) = entry.get("const") {
        When::Const(value.clone())
    } else if let Some(source) = entry.get("equals") {
        When::Equals(Pattern::parse(under_data(source)))
    } else if let Some(source) = entry.get("countOf") {
        When::CountOf(Pattern::parse(under_data(source)))
    } else {
        let template = entry["template"].as_str().expect("omitWhen rule");
        When::Template(pieces(template))
    };
    Omit { at, when }
}

/// A template's text and `{<row pointer>}` holes, in order.
pub(crate) fn pieces(template: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some((text, hole)) = rest.split_once('{') {
        let Some((pointer, after)) = hole.split_once('}') else {
            break;
        };
        if !text.is_empty() {
            pieces.push(Piece::Text(text.to_owned()));
        }
        pieces.push(Piece::Field(Pattern::parse(
            pointer.strip_prefix("/data").unwrap_or(pointer),
        )));
        rest = after;
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest.to_owned()));
    }
    pieces
}

impl Omit {
    /// This entry for one item at `prefix`: its field and every source below
    /// the item, or `None` when a part lies outside it (the value then stays).
    pub(crate) fn within(&self, prefix: &Pattern) -> Option<Self> {
        let when = match &self.when {
            When::Const(value) => When::Const(value.clone()),
            When::Equals(source) => When::Equals(source.below(prefix)?),
            When::CountOf(source) => When::CountOf(source.below(prefix)?),
            When::Template(pieces) => When::Template(
                pieces
                    .iter()
                    .map(|piece| match piece {
                        Piece::Text(text) => Some(Piece::Text(text.clone())),
                        Piece::Field(source) => Some(Piece::Field(source.below(prefix)?)),
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
        };
        Some(Self {
            at: self.at.below(prefix)?,
            when,
        })
    }
}
