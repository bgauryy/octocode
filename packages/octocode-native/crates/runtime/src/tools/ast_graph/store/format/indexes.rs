//! Decoded table lookup and adjacency index construction.
use super::*;

impl GraphTables {
    pub(crate) fn str(&self, id: u32) -> &str {
        self.strings.get(id as usize).map_or("", String::as_str)
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
        for (index, edge) in self.edges.iter().enumerate() {
            let slot = &mut cursor[edge.dst as usize];
            self.in_edges[*slot as usize] = index as u32;
            *slot += 1;
        }
    }

    fn index_lookups(&mut self) {
        let n = self.nodes.len();
        let mut keys = (0..n as u32).collect::<Vec<_>>();
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
        let mut names = (0..n as u32).collect::<Vec<_>>();
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
