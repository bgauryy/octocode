//! Decoded table lookup and adjacency index construction.
use super::*;

impl GraphTables {
    pub(crate) fn str(&self, id: u32) -> &str {
        self.strings.get(id as usize).map_or("", String::as_str)
    }

    /// Node ids in table order. Ids are `u32` columns; the reader's item
    /// limit and the publication preflight keep every table within them.
    pub(crate) fn node_ids(&self) -> impl Iterator<Item = u32> + use<> {
        (0..=u32::MAX).take(self.nodes.len())
    }

    /// The node table's length: the exclusive bound of its `u32` ids.
    pub(crate) fn node_bound(&self) -> Result<u32, String> {
        u32::try_from(self.nodes.len())
            .map_err(|_| "graph node table exceeds its u32 id space".into())
    }

    /// The string dictionary's length: the exclusive bound of its `u32` ids.
    pub(crate) fn string_bound(&self) -> Result<u32, String> {
        u32::try_from(self.strings.len())
            .map_err(|_| "graph string dictionary exceeds its u32 id space".into())
    }

    /// Recorded source stamps by file node.
    pub(crate) fn stamps_by_node(&self) -> HashMap<u32, octocode_engine::stamp::SourceStamp> {
        self.stamps.iter().copied().collect()
    }

    /// Computes both CSR directions and both lookup indexes. Call once after
    /// `edges` are sorted; the writer persists the lookup indexes.
    pub(crate) fn index(&mut self) {
        self.index_adjacency();
        self.index_lookups();
    }

    /// Both CSR directions from `edges` (sorted by source), in O(V + E).
    pub(crate) fn index_adjacency(&mut self) {
        let n = self.nodes.len();
        self.out_offsets = vec![0; n + 1];
        self.in_offsets = vec![0; n + 1];
        for edge in &self.edges {
            self.out_offsets[edge.src as usize + 1] += 1;
            self.in_offsets[edge.dst as usize + 1] += 1;
        }
        for i in 0..n {
            self.out_offsets[i + 1] += self.out_offsets[i];
            self.in_offsets[i + 1] += self.in_offsets[i];
        }
        let mut cursor = self.in_offsets.clone();
        self.in_edges = vec![0; self.edges.len()];
        // Edges are sorted by src, so each node's incoming list is sorted by
        // (src, kind) as well: deterministic without a second sort.
        // Edge ids are `u32` columns; the publication preflight keeps every
        // table within them, so the id range is never cut short.
        for (index, edge) in (0..=u32::MAX).zip(&self.edges) {
            let slot = &mut cursor[edge.dst as usize];
            self.in_edges[*slot as usize] = index;
            *slot += 1;
        }
    }

    fn index_lookups(&mut self) {
        let mut keys = self.node_ids().collect::<Vec<_>>();
        keys.sort_by(|a, b| {
            self.str(self.nodes[*a as usize].key)
                .cmp(self.str(self.nodes[*b as usize].key))
        });
        self.key_index = keys;
        let lower = self
            .nodes
            .iter()
            .map(|node| self.str(node.name).to_lowercase())
            .collect::<Vec<_>>();
        let mut names = self.node_ids().collect::<Vec<_>>();
        names.sort_by(|a, b| lower[*a as usize].cmp(&lower[*b as usize]).then(a.cmp(b)));
        self.name_index = names;
    }

    pub(crate) fn out(&self, node: u32) -> &[EdgeRec] {
        let start = self.out_offsets[node as usize] as usize;
        let end = self.out_offsets[node as usize + 1] as usize;
        &self.edges[start..end]
    }

    pub(crate) fn incoming(&self, node: u32) -> impl Iterator<Item = &EdgeRec> {
        let start = self.in_offsets[node as usize] as usize;
        let end = self.in_offsets[node as usize + 1] as usize;
        self.in_edges[start..end]
            .iter()
            .map(|index| &self.edges[*index as usize])
    }

    pub(crate) fn by_key(&self, key: &str) -> Option<u32> {
        self.key_index
            .binary_search_by(|id| self.str(self.nodes[*id as usize].key).cmp(key))
            .ok()
            .map(|at| self.key_index[at])
    }
}
