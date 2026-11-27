//! Flow-typed JavaScript read with the TSX grammar.

use super::*;

/// Flow-typed JS in the shape of React's `ReactHooks.js`: `import type`,
/// Flow function types (`S => S`) and annotated hooks.
const FLOW_HOOKS: &str = r#"/**
 * @flow
 */

import type {Dispatcher} from 'react-reconciler/src/ReactInternalTypes';
import ReactSharedInternals from 'shared/ReactSharedInternals';

type BasicStateAction<S> = (S => S) | S;
type Dispatch<A> = A => void;

function resolveDispatcher() {
  const dispatcher = ReactSharedInternals.H;
  if (__DEV__) {
    if (dispatcher === null) {
      console.error('Invalid hook call.');
    }
  }
  return ((dispatcher: any): Dispatcher);
}

export function useState<S>(
  initialState: (() => S) | S,
): [S, Dispatch<BasicStateAction<S>>] {
  const dispatcher = resolveDispatcher();
  return dispatcher.useState(initialState);
}

export function useReducer<S, I, A>(
  reducer: (S, A) => S,
  initialArg: I,
  init?: I => S,
): [S, Dispatch<A>] {
  const dispatcher = resolveDispatcher();
  if (__DEV__) {
    console.log('reducer');
  }
  return dispatcher.useReducer(reducer, initialArg, init);
}

export function useRef<T>(initialValue: T): {current: T} {
  const dispatcher = resolveDispatcher();
  if (__DEV__) {
    console.log('ref');
  }
  return dispatcher.useRef(initialValue);
}

export function useEffect(
  create: () => (() => void) | void,
  deps: Array<mixed> | void | null,
): void {
  if (__DEV__) {
    if (create == null) {
      console.warn('React Hook useEffect requires an effect callback.');
    }
  }
  const dispatcher = resolveDispatcher();
  return dispatcher.useEffect(create, deps);
}
"#;

fn declaration_names(raw: &str) -> Vec<String> {
    let facts: serde_json::Value = serde_json::from_str(raw).expect("facts JSON");
    facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter_map(|d| d["name"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn flow_js_declarations_list_the_exported_hooks_not_keywords() {
    let names = extract_declarations_document(FLOW_HOOKS, "ReactHooks.js")
        .expect("declarations")
        .declarations
        .into_iter()
        .map(|declaration| declaration.name)
        .collect::<Vec<_>>();
    for hook in [
        "useState",
        "useReducer",
        "useRef",
        "useEffect",
        "resolveDispatcher",
    ] {
        assert!(names.iter().any(|n| n == hook), "missing {hook}: {names:?}");
    }
    assert!(!names.iter().any(|n| n == "if"), "{names:?}");
}

#[test]
fn js_grammar_recovery_never_names_a_declaration_after_a_keyword() {
    // The plain JS grammar cannot read Flow; whatever it recovers, a
    // statement keyword must never surface as a declaration name.
    let raw = extract_graph_facts_with_extension(FLOW_HOOKS, "ReactHooks.js", "js").expect("facts");
    let names = declaration_names(&raw);
    assert!(
        !names
            .iter()
            .any(|n| STATEMENT_KEYWORD_PROBE.contains(&n.as_str())),
        "{names:?}"
    );
}
const STATEMENT_KEYWORD_PROBE: &[&str] = &["if", "for", "while", "switch", "return"];

#[test]
fn flow_js_symbols_view_is_an_outline_not_a_body_dump() {
    let outline = extract(FLOW_HOOKS, "ReactHooks.js").expect("outline");
    assert!(outline.contains("export function useState"), "{outline}");
    assert!(
        !outline.contains("dispatcher.useReducer"),
        "bodies must be elided: {outline}"
    );
    assert!(outline.len() * 2 < FLOW_HOOKS.len(), "{outline}");
}
