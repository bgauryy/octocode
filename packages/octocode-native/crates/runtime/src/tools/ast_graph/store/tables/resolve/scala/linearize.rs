//! Scala class linearization and member lookup along it.
//!
//! `L(C) = C, L(Bn) +⃗ … +⃗ L(B1)` for `C extends B1 with … with Bn`: the
//! bases' linearizations concatenated from the last base to the first,
//! keeping the last occurrence of a type that several share. A base that
//! did not resolve to exactly one Scala type is an opaque element at its
//! place; the same opaque base reached through two paths is one element.
//!
//! Member lookup takes the first type in `L` that declares the name, and
//! only when no opaque element comes before it: an unresolved base could
//! declare the name itself, so a link past it could name the wrong member.
//! [`KNOWN_STD_BASES`] are the exception: the standard library fixes their
//! members, and all of them are in [`UNIVERSAL_MEMBERS`]. A typed self type
//! (`self: T =>`) blocks even the owner's own members: `T` may overload or
//! override them.
use super::super::linker::Linker;
use super::super::*;
use std::cell::RefCell;
use std::rc::Rc;

/// One base of a Scala declaration, as linking resolved it.
pub(crate) enum ScalaBase {
    /// Exactly one Scala type, with the heritage link's confidence.
    Type(u32, Confidence),
    /// Unresolved, ambiguous, an alias, or a marker; the text as written.
    Opaque(String),
}

/// An element of a linearization: a type, or an owner's opaque base slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum Elem {
    Type(u32),
    Opaque(u32, u32),
}

/// A linearization, each element with the weakest heritage confidence on
/// its path.
type Linearization = Rc<[(Elem, Confidence)]>;

/// Memoized linearizations, and which import paths name a scanned package.
#[derive(Default)]
pub(crate) struct Linearizations {
    cache: RefCell<HashMap<u32, Linearization>>,
    visiting: RefCell<HashSet<u32>>,
    pub(super) packages: RefCell<HashMap<String, bool>>,
}

/// What member lookup along a linearization found.
pub(super) enum Lookup {
    /// The first declaring type's members, the weakest confidence up to it,
    /// and whether it is a base (not the owner itself).
    Hit(Vec<u32>, Confidence, bool),
    /// An opaque base comes first, or the first declaration is not a target.
    Blocked,
    /// Every element is known and none declares the name.
    Clear,
}

/// The engine's marker base for a typed self type.
const SELF_TYPE: &str = "<self-type>";

/// Standard bases whose members are all [`UNIVERSAL_MEMBERS`].
const KNOWN_STD_BASES: &[&str] = &[
    "Any",
    "AnyRef",
    "AnyVal",
    "Cloneable",
    "Equals",
    "Product",
    "Serializable",
    "Singleton",
    "java.io.Serializable",
    "java.lang.Cloneable",
    "scala.Equals",
    "scala.Product",
    "scala.Serializable",
];

/// Members every template inherits from `Any`/`AnyRef`, and the `Product`
/// and `Equals` members of every case class.
pub(super) const UNIVERSAL_MEMBERS: &[&str] = &[
    "!=",
    "##",
    "==",
    "asInstanceOf",
    "canEqual",
    "clone",
    "eq",
    "equals",
    "finalize",
    "getClass",
    "hashCode",
    "isInstanceOf",
    "ne",
    "notify",
    "notifyAll",
    "productArity",
    "productElement",
    "productElementName",
    "productElementNames",
    "productIterator",
    "productPrefix",
    "synchronized",
    "toString",
    "wait",
];

impl Linker<'_> {
    /// `L(owner)`; a cycle makes the revisited type opaque.
    pub(super) fn linearization(&self, owner: u32) -> Linearization {
        let memo = &self.bases.linearizations;
        if let Some(found) = memo.cache.borrow().get(&owner) {
            return Rc::clone(found);
        }
        if !memo.visiting.borrow_mut().insert(owner) {
            return Rc::from([(Elem::Opaque(owner, u32::MAX), Confidence::High)]);
        }
        let mut joined = Vec::new();
        let slots = self.bases.scala.get(&owner).map_or(&[][..], Vec::as_slice);
        for (index, slot) in slots.iter().enumerate().rev() {
            match slot {
                ScalaBase::Type(base, confidence) if self.scala_node(*base) => {
                    let inner = self.linearization(*base);
                    joined.extend(
                        inner
                            .iter()
                            .map(|(elem, weakest)| (*elem, (*weakest).max(*confidence))),
                    );
                }
                _ => joined.push((Elem::Opaque(owner, index as u32), Confidence::High)),
            }
        }
        let mut seen = HashSet::new();
        let mut kept = joined
            .into_iter()
            .rev()
            .filter(|(elem, _)| seen.insert(*elem))
            .collect::<Vec<_>>();
        kept.push((Elem::Type(owner), Confidence::High));
        kept.reverse();
        let linearization: Linearization = Rc::from(kept);
        memo.visiting.borrow_mut().remove(&owner);
        memo.cache
            .borrow_mut()
            .insert(owner, Rc::clone(&linearization));
        linearization
    }

    /// The first declaration of `name` along `L(owner)`.
    pub(super) fn linearized(
        &self,
        owner: u32,
        name: &str,
        accept: &dyn Fn(u32) -> bool,
        skip_owner: bool,
    ) -> Lookup {
        let universal = UNIVERSAL_MEMBERS.contains(&name);
        let mut weakest = Confidence::High;
        for (index, (elem, confidence)) in self.linearization(owner).iter().enumerate() {
            if skip_owner && index == 0 {
                continue;
            }
            match *elem {
                Elem::Type(ty) => {
                    weakest = weakest.max(*confidence);
                    let declared = self.declared_by(Some(ty), ty, name);
                    if !declared.is_empty() {
                        if index == 0 && self.self_typed(ty) {
                            return Lookup::Blocked;
                        }
                        let targets = declared
                            .into_iter()
                            .filter(|id| accept(*id))
                            .collect::<Vec<_>>();
                        if targets.is_empty() {
                            return Lookup::Blocked;
                        }
                        return Lookup::Hit(targets, weakest, index > 0);
                    }
                }
                Elem::Opaque(holder, slot) => {
                    if universal || !self.known_std_base(holder, slot) {
                        return Lookup::Blocked;
                    }
                }
            }
        }
        if universal {
            Lookup::Blocked
        } else {
            Lookup::Clear
        }
    }

    /// A base of `owner` (not `owner` itself) declares a term named `name`.
    /// Opaque bases are not searched.
    pub(super) fn inherits_term(&self, owner: u32, name: &str) -> bool {
        self.linearization(owner)
            .iter()
            .skip(1)
            .any(|(elem, _)| match *elem {
                Elem::Type(ty) => self
                    .declared_by(Some(ty), ty, name)
                    .into_iter()
                    .any(|id| !(self.is_type)(id)),
                Elem::Opaque(..) => false,
            })
    }

    /// `owner` declares a typed self type (`self: T =>`).
    pub(super) fn self_typed(&self, owner: u32) -> bool {
        self.bases.scala.get(&owner).is_some_and(|slots| {
            slots
                .iter()
                .any(|base| matches!(base, ScalaBase::Opaque(text) if text == SELF_TYPE))
        })
    }

    fn known_std_base(&self, holder: u32, slot: u32) -> bool {
        self.bases
            .scala
            .get(&holder)
            .and_then(|slots| slots.get(slot as usize))
            .is_some_and(|base| match base {
                ScalaBase::Opaque(text) => KNOWN_STD_BASES.contains(&text.as_str()),
                ScalaBase::Type(..) => false,
            })
    }
}
