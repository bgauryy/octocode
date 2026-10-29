//! C, C++ and CUDA linkage, as a compiler and linker see it: a prototype
//! (`int f(void);` in a header, `void draw();` in a class) declares the
//! function another file defines, so a call bound to the prototype reaches
//! that one definition; and a function with internal linkage (`static`, an
//! anonymous namespace) is visible only to its own translation unit, the
//! file and the files that `#include` it.
use super::*;

/// One function's identity: its enclosing namespaces and classes, then its
/// name (`["util", "Widget", "draw"]`).
type Identity<'s> = Vec<&'s str>;

impl<'a> Projector<'a> {
    /// Fills [`Symbols::definitions`] and drops from the graph-wide name
    /// pool what no other file can name: an internal-linkage definition
    /// (unless its file is included), and a definition whose prototype
    /// stands for it elsewhere. A prototype whose definition shares its
    /// file is redundant and leaves every pool.
    pub(super) fn c_definitions(&self, symbols: &mut Symbols<'a>) {
        let built = self.built;
        // (includer, included) file pairs.
        let includes = built
            .facts
            .iter()
            .filter(|(file, _)| link_family(self.lang(file)) == "system")
            .flat_map(|(file, facts)| {
                facts
                    .imports
                    .iter()
                    .filter_map(move |i| Some((file.as_str(), i.target.as_deref()?)))
            })
            .collect::<HashSet<_>>();
        let included = includes.iter().map(|(_, to)| *to).collect::<HashSet<_>>();
        let mut prototypes = HashMap::<Identity, Vec<u32>>::new();
        let mut definitions = HashMap::<Identity, Vec<(u32, bool)>>::new();
        let mut hidden = HashSet::new();
        for (file, facts) in &built.facts {
            if link_family(self.lang(file)) != "system" {
                continue;
            }
            let Some(index) = symbols.indexes.get(file.as_str()) else {
                continue;
            };
            for decl in &facts.declarations {
                let Some(&id) = index.decl_nodes.get(decl.id.as_str()) else {
                    continue;
                };
                let callable = symbols
                    .targets
                    .get(id as usize)
                    .is_some_and(|bits| bits & CALL_TARGET != 0);
                if !callable {
                    continue;
                }
                let Some(identity) = self.identity(id, decl) else {
                    continue;
                };
                if decl.prototype {
                    prototypes.entry(identity).or_default().push(id);
                    continue;
                }
                let internal = self.internal_linkage(id, decl);
                if internal && !included.contains(file.as_str()) {
                    hidden.insert(id);
                }
                definitions
                    .entry(identity)
                    .or_default()
                    .push((id, internal));
            }
        }
        let mut redundant = HashSet::new();
        for (identity, declared) in &prototypes {
            let Some(defined) = definitions.get(identity) else {
                continue;
            };
            for &prototype in declared {
                let file = self.nodes[prototype as usize].file;
                let mut visible = defined
                    .iter()
                    .filter(|(id, internal)| !internal || self.nodes[*id as usize].file == file)
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>();
                // Several definitions (a vendored copy, platform variants):
                // the one whose file includes the prototype's header.
                if visible.len() > 1 {
                    let header = self.file_path(prototype);
                    visible.retain(|id| includes.contains(&(self.file_path(*id), header)));
                }
                let [definition] = visible[..] else {
                    continue;
                };
                if self.nodes[definition as usize].file == file {
                    redundant.insert(prototype);
                } else {
                    symbols.definitions.insert(prototype, definition);
                    hidden.insert(definition);
                }
            }
        }
        for ids in symbols.global_names.values_mut() {
            ids.retain(|id| !hidden.contains(id) && !redundant.contains(id));
        }
        for index in symbols.indexes.values_mut() {
            for ids in index.by_name.values_mut() {
                ids.retain(|id| !redundant.contains(id));
            }
        }
    }

    /// Enclosing declaration names, an out-of-line definition's written
    /// scope (`Widget` of `void Widget::draw() {}`), then the name; `None`
    /// past a nesting no source spells.
    fn identity<'s>(&'s self, id: u32, decl: &'s Declaration) -> Option<Identity<'s>> {
        let mut path = vec![decl.name.as_str()];
        if let Some(owner) = decl.owner.as_deref() {
            path.extend(owner.rsplit("::").filter(|segment| !segment.is_empty()));
        }
        let mut parent = self.nodes[id as usize].parent;
        for _ in 0..64 {
            if parent == NONE {
                path.reverse();
                return Some(path);
            }
            let node = &self.nodes[parent as usize];
            path.push(self.strings.strings.get(node.name as usize)?.as_str());
            parent = node.parent;
        }
        None
    }

    /// A free function only its translation unit can name: not exported
    /// (`static`, an anonymous namespace) and inside no class.
    fn internal_linkage(&self, id: u32, decl: &Declaration) -> bool {
        if decl.exported || decl.owner.is_some() {
            return false;
        }
        let mut parent = self.nodes[id as usize].parent;
        while parent != NONE {
            let node = &self.nodes[parent as usize];
            if self
                .strings
                .strings
                .get(node.detail as usize)
                .map(String::as_str)
                != Some("namespace")
            {
                return false;
            }
            parent = node.parent;
        }
        true
    }

    /// The root-relative path of the file declaring node `id`.
    fn file_path(&self, id: u32) -> &str {
        let file = &self.nodes[self.nodes[id as usize].file as usize];
        self.strings
            .strings
            .get(file.key as usize)
            .map_or("", String::as_str)
    }
}
