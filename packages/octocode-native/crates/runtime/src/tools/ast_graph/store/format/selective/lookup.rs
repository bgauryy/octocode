use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Degrees {
    pub incoming: [u32; 6],
    pub outgoing: [u32; 6],
}

pub(in super::super) fn encode_lookup(encoder: &writer::Encoder<'_>) -> Result<Vec<u8>, String> {
    let t = encoder.tables;
    let mut degrees = (0..t.nodes.len())
        .map(|_| Degrees {
            incoming: [0; 6],
            outgoing: [0; 6],
        })
        .collect::<Vec<_>>();
    for edge in &t.edges {
        degrees
            .get_mut(edge.src as usize)
            .ok_or("graph edge source is invalid")?
            .outgoing[edge.kind as usize] += 1;
        degrees
            .get_mut(edge.dst as usize)
            .ok_or("graph edge target is invalid")?
            .incoming[edge.kind as usize] += 1;
    }
    let mut out = Vec::new();
    writer::put(&mut out, t.nodes.len() as u64);
    for (node, degree) in t.nodes.iter().zip(degrees) {
        out.push(node.kind as u8);
        writer::put(&mut out, u64::from(encoder.sid(node.key)));
        writer::put(&mut out, u64::from(encoder.sid(node.name)));
        for count in degree.incoming.into_iter().chain(degree.outgoing) {
            writer::put(&mut out, u64::from(count));
        }
    }
    Ok(out)
}

fn read_entry(r: &mut Reader<'_>, t: &GraphTables) -> Result<(NodeRec, Degrees), String> {
    let kind = NodeKind::from_u8(r.u8()?).ok_or("graph lookup kind is unknown")?;
    let key = r.var_u32()?;
    let name = r.var_u32()?;
    if key as usize >= t.strings.len()
        || name as usize >= t.strings.len()
        || (kind == NodeKind::File && !is_relative_key(t.str(key)))
    {
        return Err("graph lookup reference is invalid".into());
    }
    let mut degree = Degrees {
        incoming: [0; 6],
        outgoing: [0; 6],
    };
    for value in degree.incoming.iter_mut().chain(&mut degree.outgoing) {
        *value = r.var_u32()?;
        if *value as usize > LIMITS.edges {
            return Err("graph lookup degree exceeds edge limit".into());
        }
    }
    Ok((
        NodeRec {
            kind,
            key,
            name,
            flags: 0,
            detail: 0,
            file: NONE,
            parent: NONE,
            line: NONE,
            end_line: NONE,
        },
        degree,
    ))
}

pub(super) fn read_lookup(
    bytes: &[u8],
    t: &GraphTables,
    keep_degrees: bool,
) -> Result<(Vec<NodeRec>, Vec<Degrees>), String> {
    let mut r = Reader::new(bytes, "LOOK");
    let count = r.count()?;
    let mut nodes = Vec::with_capacity(count);
    let mut degrees = if keep_degrees {
        Vec::with_capacity(count)
    } else {
        Vec::new()
    };
    for _ in 0..count {
        let (node, degree) = read_entry(&mut r, t)?;
        nodes.push(node);
        if keep_degrees {
            degrees.push(degree);
        }
    }
    if r.at != bytes.len() {
        return Err("graph lookup has trailing data".into());
    }
    Ok((nodes, degrees))
}

pub(super) fn validate_indexes(t: &GraphTables) -> Result<(), String> {
    let n = t.nodes.len();
    for index in [&t.key_index, &t.name_index] {
        if index.len() != n {
            return Err("graph index population is inconsistent".into());
        }
        let mut seen = vec![false; n];
        for id in index {
            let seen = seen
                .get_mut(*id as usize)
                .ok_or("graph index reference is invalid")?;
            if *seen {
                return Err("graph index duplicates a node".into());
            }
            *seen = true;
        }
    }
    if !t
        .key_index
        .windows(2)
        .all(|p| t.str(t.nodes[p[0] as usize].key) < t.str(t.nodes[p[1] as usize].key))
        || !t.name_index.windows(2).all(|p| {
            (t.str(t.nodes[p[0] as usize].name).to_lowercase(), p[0])
                <= (t.str(t.nodes[p[1] as usize].name).to_lowercase(), p[1])
        })
    {
        return Err("graph lookup indexes are not sorted".into());
    }
    Ok(())
}

pub(in super::super) fn validate_lookup(
    sections: &Sections,
    t: &GraphTables,
) -> Result<(), String> {
    let mut r = sections.get(b"LOOK", "LOOK")?;
    if r.count()? != t.nodes.len() {
        return Err("graph lookup differs from node population".into());
    }
    for (id, expected) in t.nodes.iter().enumerate() {
        let (node, degrees) = read_entry(&mut r, t)?;
        if (node.kind, node.key, node.name) != (expected.kind, expected.key, expected.name) {
            return Err("graph lookup differs from node records".into());
        }
        let mut actual = Degrees {
            incoming: [0; 6],
            outgoing: [0; 6],
        };
        for edge in t.out(id as u32) {
            actual.outgoing[edge.kind as usize] += 1;
        }
        for edge in t.incoming(id as u32) {
            actual.incoming[edge.kind as usize] += 1;
        }
        if actual != degrees {
            return Err("graph lookup degree differs from topology".into());
        }
    }
    if r.at != r.bytes.len() {
        return Err("graph lookup has trailing data".into());
    }
    validate_indexes(t)
}
