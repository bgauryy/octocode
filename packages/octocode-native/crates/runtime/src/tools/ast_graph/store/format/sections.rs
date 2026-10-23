//! Budgeted readers for the string, node, edge and file-table sections.
use super::*;

pub(super) const MAX_DECODED_STRING_BYTES: usize = 512 * 1024 * 1024;

pub(super) const MAX_DECODED_ITEMS: usize = 5_000_000;

pub(super) const MAX_DECODED_EDGES: usize = 2_000_000;

pub(super) const MAX_SECTIONS: usize = 64;

pub(super) fn read_strings(strs: &mut Reader, tables: &mut GraphTables) -> Result<(), String> {
    read_strings_with_budget(strs, tables, MAX_DECODED_STRING_BYTES)
}

pub(super) fn read_strings_with_budget(
    strs: &mut Reader,
    tables: &mut GraphTables,
    max_decoded_bytes: usize,
) -> Result<(), String> {
    read_strings_with_limits(strs, tables, max_decoded_bytes, MAX_DECODED_ITEMS)
}

pub(super) fn read_strings_with_limits(
    strs: &mut Reader,
    tables: &mut GraphTables,
    max_decoded_bytes: usize,
    max_items: usize,
) -> Result<(), String> {
    let count = strs.count()?;
    if count > max_items {
        return Err("graph string table exceeds item limit; re-ingest a narrower scope".into());
    }
    tables.strings.reserve(count);
    let mut previous = Vec::<u8>::new();
    let mut decoded_bytes = 0usize;
    for _ in 0..count {
        let shared = strs.var()? as usize;
        let suffix = strs.var()? as usize;
        if shared > previous.len() {
            return Err("graph string table is corrupt".into());
        }
        previous.truncate(shared);
        previous.extend_from_slice(strs.take(suffix)?);
        decoded_bytes = decoded_bytes
            .checked_add(previous.len())
            .filter(|total| *total <= max_decoded_bytes)
            .ok_or("graph string table exceeds decoded byte limit; re-ingest a narrower scope")?;
        tables.strings.push(
            std::str::from_utf8(&previous)
                .map_err(|_| "graph string table is not UTF-8")?
                .to_owned(),
        );
    }
    Ok(())
}

pub(super) fn read_nodes(nodes: &mut Reader, tables: &mut GraphTables) -> Result<(), String> {
    let count = nodes.count()?;
    tables.nodes.reserve(count);
    let mut suffixed = Vec::new();
    for index in 0..count {
        let kind_byte = nodes.u8()?;
        let kind =
            NodeKind::from_u8(kind_byte & !KEY_SUFFIX).ok_or("graph node kind is unknown")?;
        let flags = nodes.u8()?;
        let key = nodes.var_u32()?;
        let name = nodes.var_u32()?;
        let detail = nodes.var_u32()?;
        let file = nodes.var_opt()?;
        let parent = nodes.var_opt()?;
        let line = nodes.var_opt()?;
        let end_line = if line == NONE {
            nodes.var_opt()?
        } else {
            match nodes.var()? {
                0 => NONE,
                delta => u32::try_from(i64::from(line) + unzigzag(delta - 1))
                    .map_err(|_| "graph node range is invalid")?,
            }
        };
        if kind_byte & KEY_SUFFIX != 0 {
            suffixed.push(index);
        }
        tables.nodes.push(NodeRec {
            kind,
            flags,
            key,
            name,
            detail,
            file,
            parent,
            line,
            end_line,
        });
    }
    let strings = tables.strings.len() as u32;
    let node_count = tables.nodes.len() as u32;
    let optional_node_ok = |id: u32| id == NONE || id < node_count;
    if !tables.nodes.iter().all(|node| {
        node.key < strings
            && node.name < strings
            && node.detail < strings
            && optional_node_ok(node.file)
            && optional_node_ok(node.parent)
    }) {
        return Err("graph node references are out of range".into());
    }
    // A file key is joined onto the snapshot root to read the file, so it
    // must name a path below it.
    if tables
        .nodes
        .iter()
        .any(|node| node.kind == NodeKind::File && !is_relative_key(tables.str(node.key)))
    {
        return Err("graph file key is not a root-relative path".into());
    }
    // Rebuild `path#suffix` keys for symbols stored as suffixes.
    for index in suffixed {
        let node = tables.nodes[index];
        if node.file == NONE {
            return Err("graph symbol key has no file".into());
        }
        let full = format!(
            "{}#{}",
            tables.str(tables.nodes[node.file as usize].key),
            tables.str(node.key)
        );
        tables.nodes[index].key = tables.strings.len() as u32;
        tables.strings.push(full);
    }
    Ok(())
}

/// A path below its root: every component is a plain name (no root,
/// drive, `.` or `..`).
pub(super) fn is_relative_key(key: &str) -> bool {
    !key.is_empty()
        && std::path::Path::new(key)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

pub(super) fn read_edges(
    edges: &mut Reader,
    tables: &mut GraphTables,
    strings: u32,
) -> Result<(), String> {
    let node_count = tables.nodes.len() as u32;
    let count = edges.count()?;
    if count > MAX_DECODED_EDGES {
        return Err("graph edge table exceeds item limit; re-ingest a narrower scope".into());
    }
    tables.edges.reserve(count);
    let mut src = 0u32;
    for _ in 0..count {
        src = src
            .checked_add(edges.var_u32()?)
            .ok_or("graph edge source overflows")?;
        let dst = u32::try_from(i64::from(src) + unzigzag(edges.var()?))
            .map_err(|_| "graph edge references are out of range")?;
        let packed = edges.u8()?;
        let kind = EdgeKind::from_u8(packed >> 2).ok_or("graph edge kind is unknown")?;
        let confidence =
            Confidence::from_u8(packed & 0b11).ok_or("graph edge confidence is unknown")?;
        let detail = edges.var_u32()?;
        let line = edges.var_opt()?;
        if src >= node_count || dst >= node_count || detail >= strings {
            return Err("graph edge references are out of range".into());
        }
        tables.edges.push(EdgeRec {
            src,
            dst,
            kind,
            confidence,
            detail,
            line,
        });
    }
    Ok(())
}

/// Diagnostics, file digests, components and entries.
pub(super) fn read_file_tables(
    sections: &Sections,
    tables: &mut GraphTables,
    strings: u32,
) -> Result<(), String> {
    let node_count = tables.nodes.len() as u32;
    let string_ok = |id: u32| id < strings;
    let node_ok = |id: u32| id < node_count;
    let mut diags = sections.get(b"DIAG", "DIAG")?;
    let count = diags.count()?;
    for _ in 0..count {
        let diag = DiagRec {
            file: diags.var_u32()?,
            line: diags.var_opt()?,
            code: diags.var_u32()?,
            message: diags.var_u32()?,
            target: diags.var_opt()?,
        };
        if !string_ok(diag.file)
            || !string_ok(diag.code)
            || !string_ok(diag.message)
            || diag.target != NONE && !node_ok(diag.target)
        {
            return Err("graph diagnostic references are out of range".into());
        }
        tables.diagnostics.push(diag);
    }
    for pair in sections.get(b"FDIG", "FDIG")?.var_vec()?.as_chunks::<2>().0 {
        if !node_ok(pair[0]) || !string_ok(pair[1]) {
            return Err("graph digest references are out of range".into());
        }
        tables.digests.push((pair[0], pair[1]));
    }
    // Optional sections: absent in a snapshot means empty.
    if sections.0.contains_key(b"FCMP") {
        for quad in sections.get(b"FCMP", "FCMP")?.var_vec()?.as_chunks::<4>().0 {
            if !node_ok(quad[0]) || !quad[1..].iter().all(|id| string_ok(*id)) {
                return Err("graph component references are out of range".into());
            }
            tables.components.push((quad[0], quad[1], quad[2], quad[3]));
        }
    }
    if sections.0.contains_key(b"FSTM") {
        let mut stamps = sections.get(b"FSTM", "FSTM")?;
        let count = stamps.count()?;
        tables.stamps.reserve(count);
        for _ in 0..count {
            let node = stamps.var_u32()?;
            if !node_ok(node) {
                return Err("graph stamp references are out of range".into());
            }
            let stamp = octocode_engine::stamp::SourceStamp {
                size: stamps.var()?,
                modified_ns: stamps.var()?,
                changed_ns: stamps.var()?,
                inode: stamps.var()?,
            };
            tables.stamps.push((node, stamp));
        }
    }
    if sections.0.contains_key(b"ENTR") {
        for pair in sections.get(b"ENTR", "ENTR")?.var_vec()?.as_chunks::<2>().0 {
            if !node_ok(pair[0]) || !string_ok(pair[1]) {
                return Err("graph entry references are out of range".into());
            }
            tables.entries.push((pair[0], pair[1]));
        }
    }
    Ok(())
}
