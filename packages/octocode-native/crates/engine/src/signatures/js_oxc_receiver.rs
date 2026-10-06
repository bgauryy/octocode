//! Receiver-type facts for the JS/TS lane.
//!
//! [`ReceiverScopes`] rides along the call walk in `js_oxc_calls` and gives a
//! member call `x.m()` / `this.f.m()` the type its receiver is written with:
//! a TS annotation on the binding or parameter, a `new T(..)` initializer, or
//! the class field `f` (annotated property, `constructor(private f: T)`, or
//! `this.f = new T(..)` / a typed constructor parameter). Bindings are
//! lexically scoped (functions, arrows, blocks, loops, catch clauses); an
//! unannotated binding reassigned to anything else, a destructured name, or a
//! field assigned two different ways is unknown and emits nothing.

use std::collections::HashMap;

use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;

use super::js_oxc_calls::{dotted_expression_name, ts_type_name, unwrap_expr};
use super::js_oxc_shared::property_key_name;

struct Binding {
    ty: Option<String>,
    /// Written with a type annotation: reassignment cannot change it.
    declared: bool,
}

type Fields = HashMap<String, Option<String>>;

pub(super) struct ReceiverScopes {
    scopes: Vec<HashMap<String, Binding>>,
    /// Field types of the instance `this` names; `None` where `this` is not
    /// a class instance (plain functions, static members, module code).
    this_fields: Vec<Option<Fields>>,
    /// Set by a class instance method for its own function.
    method_pending: bool,
}

impl Default for ReceiverScopes {
    fn default() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            this_fields: vec![None],
            method_pending: false,
        }
    }
}

impl ReceiverScopes {
    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// The next function is an instance method: `this` stays the class.
    pub(super) fn mark_method(&mut self) {
        self.method_pending = true;
    }

    /// Enter a non-arrow function; returns whether it rebinds `this`.
    pub(super) fn enter_function(&mut self) -> bool {
        let method = std::mem::take(&mut self.method_pending);
        self.push_scope();
        if !method {
            self.this_fields.push(None);
        }
        !method
    }

    pub(super) fn exit_function(&mut self, rebound_this: bool) {
        self.pop_scope();
        if rebound_this {
            self.this_fields.pop();
        }
    }

    pub(super) fn enter_class(&mut self, class: &Class) {
        self.this_fields.push(Some(class_fields(class)));
    }

    pub(super) fn exit_class(&mut self) {
        self.this_fields.pop();
    }

    pub(super) fn bind_declarator(&mut self, it: &VariableDeclarator) {
        let declared = it
            .type_annotation
            .as_ref()
            .map(|a| ts_type(&a.type_annotation));
        let ty = match &declared {
            Some(ty) => ty.clone(),
            None => it
                .init
                .as_ref()
                .and_then(|init| expression_type(init, None)),
        };
        self.bind_pattern(&it.id, ty, declared.is_some());
    }

    pub(super) fn bind_parameter(&mut self, it: &FormalParameter) {
        let ty = it
            .type_annotation
            .as_ref()
            .and_then(|a| ts_type(&a.type_annotation));
        self.bind_pattern(&it.pattern, ty, true);
    }

    pub(super) fn bind_catch(&mut self, it: &CatchParameter) {
        self.bind_pattern(&it.pattern, None, true);
    }

    /// `x = value`: an unannotated binding keeps its type only when the new
    /// value has the same one.
    pub(super) fn assign(&mut self, it: &AssignmentExpression) {
        if it.operator != AssignmentOperator::Assign {
            return;
        }
        let AssignmentTarget::AssignmentTargetIdentifier(target) = &it.left else {
            return;
        };
        let ty = expression_type(&it.right, None);
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(target.name.as_str()) {
                if !binding.declared && binding.ty != ty {
                    binding.ty = None;
                }
                return;
            }
        }
    }

    /// Receiver type of a call whose callee is `callee`.
    pub(super) fn receiver_type(&self, callee: &Expression) -> Option<String> {
        let object = match unwrap_expr(callee) {
            Expression::StaticMemberExpression(member) => &member.object,
            Expression::PrivateFieldExpression(member) => &member.object,
            _ => return None,
        };
        match unwrap_expr(object) {
            Expression::Identifier(identifier) => self.lookup(identifier.name.as_str()),
            Expression::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(member.property.name.as_str())
            }
            Expression::PrivateFieldExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(&format!("#{}", member.field.name.as_str()))
            }
            _ => None,
        }
    }

    fn lookup(&self, name: &str) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .and_then(|binding| binding.ty.clone())
    }

    fn this_field(&self, name: &str) -> Option<String> {
        self.this_fields.last()?.as_ref()?.get(name)?.clone()
    }

    fn bind_pattern(&mut self, pattern: &BindingPattern, ty: Option<String>, declared: bool) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if let BindingPattern::BindingIdentifier(identifier) = pattern {
            scope.insert(
                identifier.name.as_str().to_owned(),
                Binding { ty, declared },
            );
            return;
        }
        let mut names = Vec::new();
        pattern_names(pattern, &mut names);
        for name in names {
            scope.insert(
                name,
                Binding {
                    ty: None,
                    declared: true,
                },
            );
        }
    }
}

fn pattern_names(pattern: &BindingPattern, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            names.push(identifier.name.as_str().to_owned());
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                pattern_names(&property.value, names);
            }
            if let Some(rest) = &object.rest {
                pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                pattern_names(element, names);
            }
            if let Some(rest) = &array.rest {
                pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => pattern_names(&assignment.left, names),
    }
}

/// A TS type as a receiver type: a (qualified) type reference without its
/// type arguments; `T | null | undefined` reads as `T`.
fn ts_type(ty: &TSType) -> Option<String> {
    match ty {
        TSType::TSTypeReference(reference) => ts_type_name(&reference.type_name),
        TSType::TSParenthesizedType(inner) => ts_type(&inner.type_annotation),
        TSType::TSUnionType(union) => {
            let mut rest = union.types.iter().filter(|ty| {
                !matches!(ty, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_))
            });
            match (rest.next(), rest.next()) {
                (Some(only), None) => ts_type(only),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `new T(..)`, `value as T`, or an identifier typed in `parameters`.
fn expression_type(expression: &Expression, parameters: Option<&Fields>) -> Option<String> {
    if let Expression::TSAsExpression(cast) = expression {
        return ts_type(&cast.type_annotation);
    }
    match unwrap_expr(expression) {
        Expression::NewExpression(construct) => dotted_expression_name(&construct.callee),
        Expression::Identifier(identifier) => parameters?.get(identifier.name.as_str())?.clone(),
        _ => None,
    }
}

fn is_nullish(expression: &Expression) -> bool {
    match unwrap_expr(expression) {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(identifier) => identifier.name == "undefined",
        _ => false,
    }
}

/// Record `ty` for `name`; a second, different type makes it unknown.
/// A field's annotated type is declared; otherwise a non-nullish initializer
/// contributes its constructed type to the inferred fields.
fn type_field(
    (declared, inferred): (&mut Fields, &mut Fields),
    key: &PropertyKey,
    annotation: Option<&TSTypeAnnotation>,
    value: Option<&Expression>,
) {
    let Some((name, _)) = property_key_name(key) else {
        return;
    };
    if let Some(annotation) = annotation {
        declared.insert(name, ts_type(&annotation.type_annotation));
    } else if let Some(value) = value
        && !is_nullish(value)
    {
        merge(inferred, name, expression_type(value, None));
    }
}

fn merge(fields: &mut Fields, name: String, ty: Option<String>) {
    match fields.get_mut(&name) {
        Some(existing) if *existing != ty => *existing = None,
        Some(_) => {}
        None => {
            fields.insert(name, ty);
        }
    }
}

/// Instance field types of `class`. Annotations (`f: T`, parameter
/// properties) are declared and win; otherwise every `this.f = …` in the
/// class body and the field initializer must agree on one constructed type.
fn class_fields(class: &Class) -> Fields {
    let mut declared = Fields::new();
    let mut inferred = Fields::new();
    let mut constructor_parameters = Fields::new();
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(property) if !property.r#static => type_field(
                (&mut declared, &mut inferred),
                &property.key,
                property.type_annotation.as_deref(),
                property.value.as_ref(),
            ),
            ClassElement::AccessorProperty(property) if !property.r#static => type_field(
                (&mut declared, &mut inferred),
                &property.key,
                property.type_annotation.as_deref(),
                None,
            ),
            ClassElement::MethodDefinition(method)
                if method.kind == MethodDefinitionKind::Constructor =>
            {
                for parameter in &method.value.params.items {
                    let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
                        continue;
                    };
                    let ty = parameter
                        .type_annotation
                        .as_ref()
                        .and_then(|a| ts_type(&a.type_annotation));
                    let name = identifier.name.as_str().to_owned();
                    if parameter.accessibility.is_some()
                        || parameter.readonly
                        || parameter.r#override
                    {
                        declared.insert(name.clone(), ty.clone());
                    }
                    constructor_parameters.insert(name, ty);
                }
            }
            _ => {}
        }
    }
    let mut assignments = ThisAssignments {
        parameters: None,
        fields: &mut inferred,
    };
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) if !method.r#static => {
                let Some(body) = &method.value.body else {
                    continue;
                };
                assignments.parameters = (method.kind == MethodDefinitionKind::Constructor)
                    .then_some(&constructor_parameters);
                assignments.visit_function_body(body);
            }
            ClassElement::PropertyDefinition(property) if !property.r#static => {
                if let Some(Expression::ArrowFunctionExpression(arrow)) = &property.value {
                    assignments.parameters = None;
                    assignments.visit_arrow_function_body(&arrow.body);
                }
            }
            _ => {}
        }
    }
    inferred.extend(declared);
    inferred
}

/// Collects `this.f = value` inside one method body; nested non-arrow
/// functions and classes rebind `this` and are skipped.
struct ThisAssignments<'f, 'p> {
    parameters: Option<&'p Fields>,
    fields: &'f mut Fields,
}

impl<'a> VisitJs<'a> for ThisAssignments<'_, '_> {
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_class(&mut self, _it: &Class<'a>) {}

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        let name = match &it.left {
            AssignmentTarget::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                Some(member.property.name.as_str().to_owned())
            }
            AssignmentTarget::PrivateFieldExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                Some(format!("#{}", member.field.name.as_str()))
            }
            _ => None,
        };
        if let Some(name) = name
            && !is_nullish(&it.right)
        {
            let ty = if it.operator == AssignmentOperator::Assign {
                expression_type(&it.right, self.parameters)
            } else {
                None
            };
            merge(self.fields, name, ty);
        }
        walk_js::walk_assignment_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    fn receivers(source: &str, path: &str) -> BTreeMap<String, Vec<Option<String>>> {
        let json =
            super::super::js_oxc::tests::extract_graph_facts(source, path).expect("graph facts");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut out: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
        for call in value["calls"].as_array().unwrap() {
            let callee = call["callee"].as_str().unwrap();
            let method = callee.rsplit('.').next().unwrap().to_owned();
            let receiver = call["receiverType"].as_str().map(str::to_owned);
            out.entry(method).or_default().push(receiver);
        }
        out
    }

    fn assert_receivers(source: &str, path: &str, expected: &[(&str, Option<&str>)]) {
        let actual = receivers(source, path);
        for (method, receiver) in expected {
            let found = actual
                .get(*method)
                .map(|items| items.iter().map(Option::as_deref).collect::<Vec<_>>());
            assert_eq!(
                found,
                Some(vec![*receiver]),
                "{path}: receiverType of `{method}` in {actual:?}"
            );
        }
    }

    #[test]
    fn ts_receiver_types_come_from_annotations_constructors_and_class_fields() {
        let source = r#"
class Svc {
  private repo: Repo;
  cache = new Cache();
  #secret: Vault;
  constructor(private readonly store: Store, db: Db, plain) {
    this.conn = new Conn();
    this.db = db;
    this.plain = plain;
  }
  run(param: Param, list: Item[], maybe: Opt | undefined) {
    const s = new Store();
    let t: Other = make();
    const u = new ns.Thing<number>();
    s.saveA();
    t.loadB();
    u.goC();
    param.useD();
    list.mapE();
    this.repo.findF();
    this.cache.getG();
    this.store.putH();
    this.conn.openI();
    this.db.queryJ();
    this.plain.plainK();
    this.#secret.unlockL();
    maybe.unionM();
    {
      const s = make();
      s.innerN();
    }
    s.outerO();
    function inner() {
      this.repo.detachedP();
    }
    const arrow = () => this.repo.arrowQ();
    const g = (p: Pool) => p.takeR();
    unknown.nopeS();
    this.ownT();
  }
}
"#;
        assert_receivers(
            source,
            "src/svc.ts",
            &[
                ("saveA", Some("Store")),
                ("loadB", Some("Other")),
                ("goC", Some("ns.Thing")),
                ("useD", Some("Param")),
                ("mapE", None),
                ("findF", Some("Repo")),
                ("getG", Some("Cache")),
                ("putH", Some("Store")),
                ("openI", Some("Conn")),
                ("queryJ", Some("Db")),
                ("plainK", None),
                ("unlockL", Some("Vault")),
                ("unionM", Some("Opt")),
                ("innerN", None),
                ("outerO", Some("Store")),
                ("detachedP", None),
                ("arrowQ", Some("Repo")),
                ("takeR", Some("Pool")),
                ("nopeS", None),
                ("ownT", None),
            ],
        );
    }

    #[test]
    fn ts_callee_stays_as_written() {
        let json = super::super::js_oxc::tests::extract_graph_facts(
            "function f(x: Store) { x.save(); }",
            "a.ts",
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let call = &value["calls"][0];
        assert_eq!(call["callee"], "x.save");
        assert_eq!(call["receiverType"], "Store");
    }

    #[test]
    fn js_receiver_types_come_from_constructors_and_constructor_fields() {
        let source = r#"
class Svc {
  constructor() {
    this.store = new Store();
    this.twice = new A();
    this.twice = new B();
  }
  run(x) {
    const s = new Store();
    s.saveA();
    this.store.putB();
    x.useC();
    let t = new Thing();
    t = make();
    t.reboundD();
    this.twice.bothE();
  }
}
function free() {
  const m = new Map();
  m.getF();
  let r = new Reader();
  r.readG();
}
const top = new Top();
top.topH();
"#;
        assert_receivers(
            source,
            "src/svc.js",
            &[
                ("saveA", Some("Store")),
                ("putB", Some("Store")),
                ("useC", None),
                ("reboundD", None),
                ("bothE", None),
                ("getF", Some("Map")),
                ("readG", Some("Reader")),
                ("topH", Some("Top")),
            ],
        );
    }
}
