//! What a union's branches declare, for choosing the branch a failed value
//! meant: which fields select among them, and how many of the value's
//! fields a branch does not describe.
use super::super::literals::equivalent;
use super::branch_object;
use super::selectors::selector_literals;
use serde_json::{Map, Value};

/// The object forms a branch stands for: a nested `anyOf`/`oneOf` (through
/// `$ref`) contributes each of its own branches.
pub(super) fn alternatives<'a>(root: &'a Value, branch: &'a Value) -> Vec<&'a Value> {
    let target = branch_object(root, branch);
    match ["anyOf", "oneOf"]
        .iter()
        .find_map(|keyword| target.get(*keyword).and_then(Value::as_array))
    {
        Some(nested) => nested
            .iter()
            .flat_map(|branch| alternatives(root, branch))
            .collect(),
        None => vec![target],
    }
}

/// The properties one object form declares, its `allOf` parts included.
fn properties<'a>(root: &'a Value, form: &'a Value, out: &mut Vec<&'a Map<String, Value>>) {
    let form = branch_object(root, form);
    if let Some(declared) = form.get("properties").and_then(Value::as_object) {
        out.push(declared);
    }
    for part in form
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        properties(root, part, out);
    }
}

fn declared<'a>(root: &'a Value, form: &'a Value) -> Vec<&'a Map<String, Value>> {
    let mut out = Vec::new();
    properties(root, form, &mut out);
    out
}

/// The fields one object form requires, its `allOf` parts included.
fn required<'a>(root: &'a Value, form: &'a Value, out: &mut Vec<&'a str>) {
    let form = branch_object(root, form);
    out.extend(
        form.get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str),
    );
    for part in form
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        required(root, part, out);
    }
}

/// Whether a form's literal fields accept the value's: each literal field
/// the value sends must hold one of the form's literals. A literal the value
/// omits excludes no form: the caller may still send it, so the fields of a
/// form it selects (`unique` of `view: "matchOnly"`) are fields the row may
/// author.
fn selectors_fit(root: &Value, form: &Value, value: &Value) -> bool {
    declared(root, form)
        .into_iter()
        .flatten()
        .all(|(name, schema)| {
            let Some(literals) = selector_literals(root, schema) else {
                return true;
            };
            value
                .get(name)
                .is_none_or(|actual| literals.iter().any(|literal| equivalent(literal, actual)))
        })
}

/// The fields a caller may author in the forms `value` still fits: forms
/// whose literal fields accept the value's and that declare every other
/// field it sends that some form declares; failing that, the forms whose
/// literal fields accept it. `None` when no form's literal fields do. An
/// unknown field's allowed set is then the row's form, not the one form
/// branch scoring picked (localFetch `view:"content"` reads a whole file,
/// ranges or matches).
pub(super) fn compatible_fields(
    root: &Value,
    branches: &[Value],
    value: &Value,
) -> Option<Vec<String>> {
    let object = value.as_object()?;
    let forms = branches
        .iter()
        .flat_map(|branch| alternatives(root, branch))
        .map(|form| (selectors_fit(root, form, value), declared(root, form)))
        .collect::<Vec<_>>();
    let declares = |maps: &[&Map<String, Value>], name: &str| {
        maps.iter().any(|declared| declared.contains_key(name))
    };
    let sent = object
        .keys()
        .filter(|name| forms.iter().any(|(_, maps)| declares(maps, name)))
        .collect::<Vec<_>>();
    // A value sending no declared field has chosen no form: each one fits.
    let fit = forms
        .iter()
        .filter(|(fits, _)| *fits || sent.is_empty())
        .collect::<Vec<_>>();
    let fitting = fit
        .iter()
        .copied()
        .filter(|(_, maps)| sent.iter().all(|name| declares(maps, name)))
        .collect::<Vec<_>>();
    let chosen = if fitting.is_empty() { fit } else { fitting };
    if chosen.is_empty() {
        return None;
    }
    let mut fields = chosen
        .iter()
        .flat_map(|(_, maps)| maps.iter().copied().flatten())
        .filter(|(_, property)| super::super::schema::authored_property(root, property))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    fields.sort();
    fields.dedup();
    Some(fields)
}

/// Whether `value` is an object that sends no field any form of `branches`
/// declares: nothing in it chooses a form, so a form that scoring picks
/// (the one lacking fewest fields) is a guess, not the value's form.
pub(super) fn chooses_no_form(root: &Value, branches: &[Value], value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        let maps = branches
            .iter()
            .flat_map(|branch| alternatives(root, branch))
            .flat_map(|form| declared(root, form))
            .collect::<Vec<_>>();
        !object
            .keys()
            .any(|name| maps.iter().any(|declared| declared.contains_key(name)))
    })
}

/// The fields each form of `branches` requires, when the forms share none
/// (`tool` and `query`, or `url`): which of them a value sends is what
/// chooses its form. A set holding another one is that form plus more, so
/// only the smaller is named. `None` with fewer than two such sets, a form
/// requiring nothing, or two sets sharing a field.
pub(super) fn disjoint_forms(root: &Value, branches: &[Value]) -> Option<Vec<Vec<String>>> {
    let mut sets: Vec<Vec<String>> = Vec::new();
    for form in branches
        .iter()
        .flat_map(|branch| alternatives(root, branch))
    {
        let mut needed = Vec::new();
        required(root, form, &mut needed);
        // Declaration order, the order the fields are named in.
        let mut set: Vec<String> = Vec::new();
        for field in needed {
            if !set.iter().any(|known| known == field) {
                set.push(field.to_owned());
            }
        }
        if set.is_empty() {
            return None;
        }
        if !sets
            .iter()
            .any(|known| known.len() == set.len() && known.iter().all(|f| set.contains(f)))
        {
            sets.push(set);
        }
    }
    let contains = |outer: &[String], inner: &[String]| inner.iter().all(|f| outer.contains(f));
    let minimal = sets
        .iter()
        .filter(|set| {
            !sets
                .iter()
                .any(|other| other != *set && contains(set, other))
        })
        .cloned()
        .collect::<Vec<_>>();
    let shared = minimal.iter().enumerate().any(|(index, set)| {
        minimal[index + 1..]
            .iter()
            .any(|other| other.iter().any(|field| set.contains(field)))
    });
    (minimal.len() > 1 && !shared).then_some(minimal)
}

/// The fields that select among `branches`: forms pin each to literals
/// (`const`/`enum`), and not all to the same set. A literal on any other
/// field (a `role` only one form declares, or every form pins alike) is a
/// value of the form, not a choice between forms. A selector whose forms'
/// sets share no literal names one form by its value alone ([`Selectors::names_one`]):
/// `by` beside `asc`-only and `asc`-or-`desc` orders.
pub(super) fn selector_fields(root: &Value, branches: &[Value]) -> Selectors {
    let mut sets: Vec<(&str, Vec<Vec<&Value>>)> = Vec::new();
    for form in branches
        .iter()
        .flat_map(|branch| alternatives(root, branch))
    {
        for declared in declared(root, form) {
            for (name, schema) in declared {
                let schema = super::super::super::resolve_ref(schema, &root["$defs"]);
                let Some(literals) = selector_literals(root, schema) else {
                    continue;
                };
                let index = sets
                    .iter()
                    .position(|(field, _)| field == name)
                    .unwrap_or_else(|| {
                        sets.push((name, Vec::new()));
                        sets.len() - 1
                    });
                let known = &mut sets[index].1;
                if !known.iter().any(|set| same_literals(set, &literals)) {
                    known.push(literals);
                }
            }
        }
    }
    let disjoint = |distinct: &[Vec<&Value>]| {
        distinct.iter().enumerate().all(|(index, set)| {
            distinct[index + 1..].iter().all(|other| {
                !set.iter()
                    .any(|literal| other.iter().any(|known| equivalent(literal, known)))
            })
        })
    };
    let mut selectors = Selectors::default();
    for (field, distinct) in sets.into_iter().filter(|(_, distinct)| distinct.len() > 1) {
        if disjoint(&distinct) {
            selectors.naming.push(field.to_owned());
        }
        selectors.fields.push(field.to_owned());
        selectors.literal_sets.push((
            field.to_owned(),
            distinct
                .iter()
                .map(|set| set.iter().map(|literal| (*literal).clone()).collect())
                .collect(),
        ));
    }
    selectors
}

/// A union's selector fields ([`selector_fields`]), and those of them whose
/// value names one form.
#[derive(Debug, Default)]
pub(super) struct Selectors {
    fields: Vec<String>,
    naming: Vec<String>,
    /// Per selector, the literal set each distinct form pins it to.
    literal_sets: Vec<(String, Vec<Vec<Value>>)>,
}

impl Selectors {
    /// Whether `field` is a selector whose forms' literal sets share no
    /// value: a mismatch there says the value is of another form, where a
    /// mismatch on a selector whose sets overlap (an `order` one form takes
    /// as `asc` alone) may be the value's error within its own form.
    pub(super) fn names_one(&self, field: &str) -> bool {
        self.naming.iter().any(|named| named == field)
    }

    /// Whether some form pins `field` to a set holding `value`.
    pub(super) fn value_known(&self, field: &str, value: &Value) -> bool {
        self.literal_sets
            .iter()
            .filter(|(name, _)| name == field)
            .any(|(_, sets)| {
                sets.iter()
                    .any(|set| set.iter().any(|literal| equivalent(value, literal)))
            })
    }

    /// Whether `value`, sent for `field`, is the one literal a single form
    /// pins the field to (`operation: "stats"` beside forms sharing other
    /// operations): the row is of that form by that value alone, so a
    /// mismatch on it weighs like one on a field that names a form by
    /// itself. A value merely inside a form's larger set (`order: "desc"`
    /// beside `asc` alone) is that form's own value, not its name.
    pub(super) fn value_names_one(&self, field: &str, value: &Value) -> bool {
        self.literal_sets
            .iter()
            .filter(|(name, _)| name == field)
            .any(|(_, sets)| {
                let mut holding = sets
                    .iter()
                    .filter(|set| set.iter().any(|literal| equivalent(value, literal)));
                matches!(
                    (holding.next(), holding.next()),
                    (Some(set), None) if set.len() == 1
                )
            })
    }
}

impl std::ops::Deref for Selectors {
    type Target = [String];

    fn deref(&self) -> &[String] {
        &self.fields
    }
}

fn same_literals(left: &[&Value], right: &[&Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|literal| right.iter().any(|other| equivalent(literal, other)))
}

/// How many of the value's fields no form of `branch` declares: the fewer,
/// the more specifically the branch describes the value (an open branch
/// reports no unknown field for them).
pub(super) fn undeclared_fields(root: &Value, branch: &Value, value: &Value) -> usize {
    let Some(object) = value.as_object() else {
        return 0;
    };
    let forms = alternatives(root, branch)
        .into_iter()
        .flat_map(|form| declared(root, form))
        .collect::<Vec<_>>();
    object
        .keys()
        .filter(|key| !forms.iter().any(|declared| declared.contains_key(*key)))
        .count()
}

#[cfg(test)]
mod selector_tests {
    use serde_json::json;

    /// The issue a query's validation reports at `queries[0].sort`.
    fn sort_issue(tool: &str, query: serde_json::Value) -> (String, String) {
        let error = crate::contracts::validate(tool, json!({ "queries": [query] }))
            .expect_err("the sort is refused");
        let issue = error
            .issues
            .iter()
            .find(|issue| issue.path.iter().any(|part| part == "sort"))
            .unwrap_or_else(|| panic!("a sort issue: {error:?}"));
        (issue.path.join("."), issue.message.clone())
    }

    /// A sort whose `by` only one form takes, with an `order` that form
    /// refuses, is refused at its `order`, whichever form the union lists
    /// first: `by` names the form (its sets share no value), and `order`
    /// (`asc` alone beside `asc` or `desc`) is a value within it.
    #[test]
    fn the_field_whose_sets_share_no_value_chooses_the_form() {
        for (tool, query, only) in [
            (
                "structureSearch",
                json!({"operation":"files","path":".","sort":{"by":"traversal","order":"desc"}}),
                "asc",
            ),
            (
                "localSearch",
                json!({"path":".","matchString":["x"],"sort":{"by":"traversal","order":"desc"}}),
                "asc",
            ),
            (
                "ghSearchRepo",
                json!({"operation":"search","keywords":["x"],"sort":{"by":"relevance","order":"asc"}}),
                "desc",
            ),
        ] {
            let (path, message) = sort_issue(tool, query);
            assert!(path.ends_with("sort.order"), "{tool}: {path}: {message}");
            assert!(
                message.contains(&format!("allowed: {only}")),
                "{tool}: {message}"
            );
        }
        // A selector whose sets overlap still chooses among the forms the
        // naming ones leave: a Maven resolve fails at its name, not at an
        // ecosystem another resolve form lacks.
        let error = crate::contracts::validate(
            "artifactSearch",
            json!({"queries":[{"operation":"resolve","ecosystem":"maven","packageName":"log4j-core"}]}),
        )
        .expect_err("a Maven name needs group:artifact");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path.last().map(String::as_str) == Some("packageName")),
            "{error:?}"
        );
        assert!(
            !error
                .issues
                .iter()
                .any(|issue| issue.path.last().map(String::as_str) == Some("ecosystem")),
            "{error:?}"
        );
        // A `by` no form takes is refused at `by`, listing every form's.
        let (path, message) = sort_issue(
            "structureSearch",
            json!({"operation":"files","path":".","sort":{"by":"size","order":"asc"}}),
        );
        assert!(path.ends_with("sort.by"), "{path}: {message}");
        assert!(
            message.contains("traversal") && message.contains("bytes"),
            "{message}"
        );
    }
}
