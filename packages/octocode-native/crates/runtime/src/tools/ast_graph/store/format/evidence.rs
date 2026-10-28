//! Persist occurrence evidence independently from compact adjacency.
use super::*;
use writer::{Encoder, put};

impl Encoder<'_> {
    fn provenance(&self, out: &mut Vec<u8>, provenance: ProvenanceRec) {
        for value in [
            self.sid(provenance.source_digest),
            provenance.source_start,
            provenance.source_end,
            provenance.source_origin,
            self.sid(provenance.derivation_version),
        ] {
            put(out, u64::from(value));
        }
    }

    pub(super) fn declarations(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, self.tables.declarations.len() as u64);
        for decl in &self.tables.declarations {
            for value in [
                self.sid(decl.test_identity),
                decl.node,
                self.sid(decl.occurrence),
                self.sid(decl.source_digest),
                decl.source_start,
                decl.source_end,
                decl.source_origin,
                decl.selection_start,
                decl.selection_end,
                decl.selection_origin,
                self.sid(decl.assessment),
            ] {
                put(&mut out, u64::from(value));
            }
        }
        out
    }
    pub(super) fn witnesses(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, self.tables.witnesses.len() as u64);
        for witness in &self.tables.witnesses {
            let e = witness.edge;
            for value in [
                e.src,
                e.dst,
                e.kind as u32,
                e.confidence as u32,
                self.sid(e.detail),
                e.line,
                u32::from(e.test),
                self.sid(witness.occurrence),
            ] {
                put(&mut out, u64::from(value));
            }
            self.provenance(&mut out, witness.provenance);
            put(
                &mut out,
                u64::from(if witness.argument == NONE {
                    NONE
                } else {
                    self.sid(witness.argument)
                }),
            );
        }
        out
    }

    pub(super) fn unresolved(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, self.tables.unresolved.len() as u64);
        for site in &self.tables.unresolved {
            for value in [
                site.relation as u32,
                site.src,
                self.sid(site.occurrence),
                self.sid(site.callee),
                site.line,
                self.sid(site.reason),
            ] {
                put(&mut out, u64::from(value));
            }
            self.provenance(&mut out, site.provenance);
            put(
                &mut out,
                u64::from(if site.argument == NONE {
                    NONE
                } else {
                    self.sid(site.argument)
                }),
            );
        }
        out
    }
}

pub(super) fn read_evidence(
    sections: &Sections,
    t: &mut GraphTables,
    strings: u32,
) -> Result<(), String> {
    let mut r = sections.get(b"DECL", "DECL")?;
    let count = r.count()?;
    for _ in 0..count {
        let decl = DeclarationRec {
            test_identity: r.var_u32()?,
            node: r.var_u32()?,
            occurrence: r.var_u32()?,
            source_digest: r.var_u32()?,
            source_start: r.var_u32()?,
            source_end: r.var_u32()?,
            source_origin: r.var_u32()?,
            selection_start: r.var_u32()?,
            selection_end: r.var_u32()?,
            selection_origin: r.var_u32()?,
            assessment: r.var_u32()?,
        };
        if decl.test_identity >= strings
            || decl.node as usize >= t.nodes.len()
            || decl.occurrence >= strings
            || decl.source_digest >= strings
            || decl.assessment >= strings
            || decl.source_start > decl.source_end
            || (decl.source_start == NONE) != (decl.source_end == NONE)
            || t.nodes
                .get(decl.node as usize)
                .is_none_or(|node| node.kind != NodeKind::Symbol)
            || t.declarations
                .last()
                .is_some_and(|previous| previous.node >= decl.node)
            || serde_json::from_str::<Option<octocode_engine::graph::GraphTestIdentity>>(
                t.str(decl.test_identity),
            )
            .is_err()
            || serde_json::from_str::<octocode_engine::graph::GraphParseAssessment>(
                t.str(decl.assessment),
            )
            .is_err()
        {
            return Err(
                "graph declaration evidence contains an invalid reference or assessment".into(),
            );
        }
        t.sources
            .check_range(decl.source_origin, decl.source_start, decl.source_end)?;
        t.sources.check_range(
            decl.selection_origin,
            decl.selection_start,
            decl.selection_end,
        )?;
        t.sources
            .check_digest(decl.source_origin, t.str(decl.source_digest))?;
        t.declarations.push(decl);
    }
    let mut r = sections.get(b"WITN", "WITN")?;
    let count = r.count()?;
    for _ in 0..count {
        let src = r.var_u32()?;
        let dst = r.var_u32()?;
        let kind = r.var_u32()?;
        let confidence = r.var_u32()?;
        let detail = r.var_u32()?;
        let line = r.var_u32()?;
        let test = r.var_u32()?;
        let occurrence = r.var_u32()?;
        let provenance = read_provenance(&mut r, t, strings)?;
        let argument = r.var_u32()?;
        if src as usize >= t.nodes.len()
            || dst as usize >= t.nodes.len()
            || detail >= strings
            || occurrence >= strings
            || test > 1
            || !source_node(t, src)
            || line == 0
        {
            return Err("graph witness contains an invalid reference".into());
        }
        let kind = u8::try_from(kind)
            .ok()
            .and_then(EdgeKind::from_u8)
            .ok_or("graph witness has an unknown relation")?;
        let confidence = u8::try_from(confidence)
            .ok()
            .and_then(Confidence::from_u8)
            .ok_or("graph witness has unknown confidence")?;
        if kind == EdgeKind::Passes && line == NONE {
            return Err("passes witness has no source line".into());
        }
        arguments::validate(t, argument, kind, provenance, None)?;
        t.witnesses.push(WitnessRec {
            argument,
            provenance,
            occurrence,
            edge: EdgeRec {
                src,
                dst,
                kind,
                confidence,
                detail,
                line,
                test: test != 0,
            },
        });
    }
    let mut r = sections.get(b"UNRS", "UNRS")?;
    let count = r.count()?;
    for _ in 0..count {
        let relation = u8::try_from(r.var_u32()?)
            .ok()
            .and_then(EdgeKind::from_u8)
            .filter(|kind| matches!(kind, EdgeKind::Calls | EdgeKind::Passes))
            .ok_or("unresolved site has an unsupported relation")?;
        let site = UnresolvedRec {
            relation,
            src: r.var_u32()?,
            occurrence: r.var_u32()?,
            callee: r.var_u32()?,
            line: r.var_u32()?,
            reason: r.var_u32()?,
            provenance: read_provenance(&mut r, t, strings)?,
            argument: r.var_u32()?,
        };
        if site.src as usize >= t.nodes.len()
            || site.occurrence >= strings
            || site.callee >= strings
            || site.reason >= strings
            || !source_node(t, site.src)
            || site.line == 0
            || site.line == NONE
        {
            return Err("graph unresolved site contains an invalid reference".into());
        }
        arguments::validate(
            t,
            site.argument,
            site.relation,
            site.provenance,
            Some(t.str(site.reason)),
        )?;
        t.unresolved.push(site);
    }
    Ok(())
}

fn read_provenance(
    r: &mut Reader<'_>,
    t: &GraphTables,
    strings: u32,
) -> Result<ProvenanceRec, String> {
    let provenance = ProvenanceRec {
        source_digest: r.var_u32()?,
        source_start: r.var_u32()?,
        source_end: r.var_u32()?,
        source_origin: r.var_u32()?,
        derivation_version: r.var_u32()?,
    };
    if provenance.source_digest >= strings
        || provenance.derivation_version >= strings
        || provenance.source_start > provenance.source_end
        || (provenance.source_start == NONE) != (provenance.source_end == NONE)
        || t.str(provenance.derivation_version).is_empty()
    {
        return Err("graph occurrence provenance contains an invalid reference or range".into());
    }
    t.sources.check_range(
        provenance.source_origin,
        provenance.source_start,
        provenance.source_end,
    )?;
    t.sources
        .check_digest(provenance.source_origin, t.str(provenance.source_digest))?;
    Ok(provenance)
}

fn source_node(t: &GraphTables, id: u32) -> bool {
    t.nodes.get(id as usize).is_some_and(|node| {
        matches!(node.kind, NodeKind::File | NodeKind::Symbol)
            && t.nodes
                .get(node.file as usize)
                .is_some_and(|file| file.kind == NodeKind::File)
    })
}
