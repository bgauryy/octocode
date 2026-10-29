//! Budgeted readers for the string, node, edge and file-table sections.
use super::*;

/// Appends one front-coded dictionary section; `limits` bound the whole
/// dictionary, including strings earlier sections decoded.
pub(super) fn read_strings(
    strs: &mut Reader,
    tables: &mut GraphTables,
    limits: &Limits,
) -> Result<(), String> {
    let (max_decoded_bytes, max_items) = (limits.string_bytes, limits.table_items);
    let count = strs.count()?;
    if count > max_items.saturating_sub(tables.strings.len()) {
        return Err("graph string table exceeds item limit; re-ingest a narrower scope".into());
    }
    tables.strings.reserve(count);
    let mut previous = Vec::<u8>::new();
    let mut decoded_bytes = tables.strings.iter().map(String::len).sum::<usize>();
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

pub(super) fn read_node(nodes: &mut Reader) -> Result<NodeRec, String> {
    let kind = NodeKind::from_u8(nodes.u8()?).ok_or("graph node kind is unknown")?;
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
    Ok(NodeRec {
        kind,
        flags,
        key,
        name,
        detail,
        file,
        parent,
        line,
        end_line,
    })
}

pub(super) fn validate_node(node: &NodeRec, tables: &GraphTables) -> Result<(), String> {
    let string_ok = |id: u32| (id as usize) < tables.strings.len();
    if !string_ok(node.key)
        || !string_ok(node.name)
        || !string_ok(node.detail)
        || (node.file != NONE
            && tables
                .nodes
                .get(node.file as usize)
                .is_none_or(|file| file.kind != NodeKind::File))
        || (node.parent != NONE
            && tables
                .nodes
                .get(node.parent as usize)
                .is_none_or(|parent| parent.kind != NodeKind::Symbol))
        || (node.kind == NodeKind::Symbol && node.file == NONE)
    {
        return Err("graph node references are out of range or have an invalid kind".into());
    }
    if node.kind == NodeKind::File && !is_relative_key(tables.str(node.key)) {
        return Err("graph file key is not a root-relative path".into());
    }
    Ok(())
}

pub(super) fn read_nodes(nodes: &mut Reader, tables: &mut GraphTables) -> Result<(), String> {
    let count = nodes.count()?;
    tables.nodes.reserve(count);
    for _ in 0..count {
        tables.nodes.push(read_node(nodes)?);
    }
    for node in &tables.nodes {
        validate_node(node, tables)?;
    }
    if nodes.at != nodes.bytes.len() {
        return Err("graph node section has trailing records".into());
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
    if count > LIMITS.edges {
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
        let kind =
            EdgeKind::from_u8((packed & !EDGE_TEST) >> 2).ok_or("graph edge kind is unknown")?;
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
            test: packed & EDGE_TEST != 0,
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
    let file_ok = |id: u32| {
        tables
            .nodes
            .get(id as usize)
            .is_some_and(|node| node.kind == NodeKind::File)
    };
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
    for pair in records::<2>(&sections.get(b"FDIG", "FDIG")?.var_vec()?, "FDIG")? {
        if !file_ok(pair[0]) || !string_ok(pair[1]) {
            return Err("graph digest references are out of range".into());
        }
        tables.digests.push((pair[0], pair[1]));
    }
    // Optional sections: absent in a snapshot means empty.
    if sections.0.contains_key(b"FCMP") {
        for quad in records::<4>(&sections.get(b"FCMP", "FCMP")?.var_vec()?, "FCMP")? {
            if !file_ok(quad[0]) || !quad[1..].iter().all(|id| string_ok(*id)) {
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
            if !file_ok(node) {
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
        for pair in records::<2>(&sections.get(b"ENTR", "ENTR")?.var_vec()?, "ENTR")? {
            if !file_ok(pair[0]) || !string_ok(pair[1]) {
                return Err("graph entry references are out of range".into());
            }
            tables.entries.push((pair[0], pair[1]));
        }
    }
    Ok(())
}

fn records<'a, const N: usize>(values: &'a [u32], section: &str) -> Result<&'a [[u32; N]], String> {
    let (records, remainder) = values.as_chunks::<N>();
    if !remainder.is_empty() {
        return Err(format!(
            "graph section {section} contains an incomplete record"
        ));
    }
    Ok(records)
}
