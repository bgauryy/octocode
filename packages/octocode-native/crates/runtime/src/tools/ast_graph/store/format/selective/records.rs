use super::*;

impl SnapshotReader {
    pub(crate) fn node(
        &mut self,
        id: u32,
        t: &GraphTables,
        cancel: &dyn CancellationCheck,
    ) -> Result<NodeRec, String> {
        let blocks = &self.blocks[b"NODE"].1;
        let block_index = blocks.partition_point(|block| block.last < id);
        let block = blocks
            .get(block_index)
            .filter(|block| block.first <= id)
            .ok_or("graph node record is absent")?;
        let (first, last, count) = (block.first, block.last, block.count);
        if self
            .node_cache
            .as_ref()
            .is_none_or(|(index, _)| *index != block_index)
        {
            let bytes = self.record_block(*b"NODE", block_index, cancel)?;
            let mut r = Reader::new(&bytes, "NODE block");
            if r.count()? != count as usize
                || u64::from(last) - u64::from(first) + 1 != u64::from(count)
            {
                return Err("graph node block count is inconsistent".into());
            }
            let mut nodes = Vec::with_capacity(count as usize);
            for index in first..=last {
                let node = read_node(&mut r)?;
                validate_node(&node, t)?;
                let lookup = t
                    .nodes
                    .get(index as usize)
                    .ok_or("graph node block reference is invalid")?;
                if (node.kind, node.key, node.name) != (lookup.kind, lookup.key, lookup.name) {
                    return Err("graph node differs from its lookup".into());
                }
                nodes.push(node);
            }
            if r.at != bytes.len() {
                return Err("graph node block has trailing records".into());
            }
            self.decoded_nodes += nodes.len();
            self.node_cache = Some((block_index, nodes));
        }
        Ok(self
            .node_cache
            .as_ref()
            .ok_or("node block cache is absent")?
            .1[(id - first) as usize])
    }

    pub(crate) fn declaration(
        &mut self,
        id: u32,
        t: &GraphTables,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<DeclarationRec>, String> {
        let blocks = &self.blocks[b"DECL"].1;
        let block_index = blocks.partition_point(|block| block.last < id);
        let Some(block) = blocks.get(block_index).filter(|block| block.first <= id) else {
            return Ok(None);
        };
        let (first, last, count) = (block.first, block.last, block.count);
        if self
            .declaration_cache
            .as_ref()
            .is_none_or(|(index, _)| *index != block_index)
        {
            // Declarations are validated against the source origins they cite.
            let bytes = self.record_block(*b"DECL", block_index, cancel)?;
            let mut r = Reader::new(&bytes, "DECL block");
            let declarations =
                evidence::read_declaration_records(&mut r, t, t.strings.len() as u32)?;
            for decl in &declarations {
                for origin in evidence::declaration_origins(decl) {
                    if origin != NONE {
                        self.load_origin(origin, cancel)?;
                    }
                }
            }
            let sources = self.sources(cancel)?;
            for decl in &declarations {
                evidence::check_declaration_sources(decl, t, sources)?;
            }
            if declarations.len() != count as usize
                || declarations.first().map(|d| d.node) != Some(first)
                || declarations.last().map(|d| d.node) != Some(last)
                || r.at != bytes.len()
            {
                return Err("graph declaration block differs from its directory".into());
            }
            self.declaration_cache = Some((block_index, declarations));
        }
        let declarations = &self
            .declaration_cache
            .as_ref()
            .ok_or("declaration block cache is absent")?
            .1;
        Ok(declarations
            .binary_search_by_key(&id, |d| d.node)
            .ok()
            .map(|index| declarations[index]))
    }
}
