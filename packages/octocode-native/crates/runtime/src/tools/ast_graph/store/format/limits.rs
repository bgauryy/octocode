//! The encoder admits only table sizes accepted by the decoder. Row counts
//! are checked on the projection, before redaction (which rewrites strings,
//! never rows); string bytes once the redacted strings are encoded.
use super::*;

pub(super) fn validate(encoder: &writer::Encoder<'_>, limits: &Limits) -> Result<(), String> {
    validate_counts(encoder.tables, limits)?;
    validate_strings(encoder, limits)?;
    let t = encoder.tables;
    for witness in &t.witnesses {
        arguments::validate(
            t,
            witness.argument,
            witness.edge.kind,
            witness.provenance,
            None,
        )?;
        if witness.edge.kind == EdgeKind::Passes && matches!(witness.edge.line, 0 | NONE) {
            return Err("passes witness has no source line".into());
        }
    }
    for site in &t.unresolved {
        arguments::validate(
            t,
            site.argument,
            site.relation,
            site.provenance,
            Some(t.str(site.reason)),
        )?;
    }
    Ok(())
}

/// The first table of `t` with more rows than a reader of `limits` admits.
pub(crate) fn validate_counts(t: &GraphTables, limits: &Limits) -> Result<(), String> {
    let items = limits.table_items;
    for (section, count, width) in [
        ("NODE", t.nodes.len(), 1),
        ("KEYX", t.key_index.len(), 1),
        ("NAMX", t.name_index.len(), 1),
        ("DIAG", t.diagnostics.len(), 1),
        ("WITN", t.witnesses.len(), 1),
        ("UNRS", t.unresolved.len(), 1),
        ("DECL", t.declarations.len(), 1),
        ("FDIG", t.digests.len(), 2),
        ("FCMP", t.components.len(), 4),
        ("ENTR", t.entries.len(), 2),
        ("FSTM", t.stamps.len(), 1),
    ] {
        if count.checked_mul(width).is_none_or(|count| count > items) {
            return Err(format!(
                "graph {section} table exceeds the reader's {items}-item limit; this is a terminal snapshot bound, ingest a narrower scope"
            ));
        }
    }
    if t.edges.len() > limits.edge_rows() {
        return Err(format!(
            "graph EDGE table exceeds the reader's {}-edge limit; this is a terminal snapshot bound, ingest a narrower scope",
            limits.edge_rows(),
        ));
    }
    Ok(())
}

/// The record dictionary's count and both dictionaries' expanded bytes.
fn validate_strings(encoder: &writer::Encoder<'_>, limits: &Limits) -> Result<(), String> {
    let (items, string_bytes) = (limits.table_items, limits.string_bytes);
    if encoder.texts.len() > items {
        return Err(format!(
            "graph STRS table exceeds the reader's {items}-item limit; this is a terminal snapshot bound, ingest a narrower scope"
        ));
    }
    encoder.texts.iter().try_fold(0_usize, |total, text| {
        total.checked_add(text.len()).filter(|bytes| *bytes <= string_bytes)
    }).ok_or_else(|| format!(
        "graph STRS table exceeds the reader's {string_bytes}-byte expanded string limit; this is a terminal snapshot bound, ingest a narrower scope"
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pre-encoding checks, at `items` rows, `edges` EDGE rows and
    /// `string_bytes` expanded string bytes.
    fn validate_with(
        encoder: &writer::Encoder<'_>,
        items: usize,
        edges: usize,
        string_bytes: usize,
    ) -> Result<(), String> {
        let limits = Limits {
            table_items: items,
            edges,
            string_bytes,
            ..LIMITS
        };
        validate_counts(encoder.tables, &limits)?;
        validate_strings(encoder, &limits)
    }

    #[test]
    fn publication_preflight_counts_flattened_file_tables() {
        for table in ["FDIG", "FCMP", "ENTR"] {
            let mut t = GraphTables {
                strings: vec!["source".into()],
                ..Default::default()
            };
            match table {
                "FDIG" => t.digests = vec![(0, 0); 2],
                "FCMP" => t.components = vec![(0, 0, 0, 0)],
                _ => t.entries = vec![(0, 0); 2],
            }
            let encoder = writer::Encoder::new(&t);
            let error = validate_with(&encoder, 3, 3, 100).unwrap_err();
            assert!(error.contains(table), "{error}");
            assert!(validate_with(&encoder, 4, 4, 100).is_ok());
        }
    }

    #[test]
    fn front_coding_does_not_hide_expanded_string_budget() {
        let prefix = "shared-prefix-".repeat(30);
        let t = GraphTables {
            strings: vec![format!("{prefix}a"), format!("{prefix}b")],
            digests: vec![(0, 0), (0, 1)],
            ..Default::default()
        };
        let encoder = writer::Encoder::new(&t);
        assert!(encoder.record_strings().len() < 500);
        let error = validate_with(&encoder, 10, 10, 500).unwrap_err();
        assert!(error.contains("expanded string limit"));
        assert!(validate_with(&encoder, 10, 10, 1000).is_ok());
    }

    #[test]
    fn occurrence_counts_are_checked_without_truncation() {
        let mut t = GraphTables {
            strings: vec!["source".into()],
            ..Default::default()
        };
        let edge = EdgeRec {
            src: 0,
            dst: 0,
            kind: EdgeKind::Calls,
            confidence: Confidence::High,
            detail: 0,
            line: 1,
            test: false,
        };
        let provenance = ProvenanceRec {
            source_digest: 0,
            source_start: 0,
            source_end: 1,
            source_origin: NONE,
            derivation_version: 0,
        };
        t.witnesses = vec![
            WitnessRec {
                argument: NONE,
                edge,
                occurrence: 0,
                provenance
            };
            3
        ];
        let error = validate_with(&writer::Encoder::new(&t), 2, 2, 100).unwrap_err();
        assert!(error.contains("WITN"));
        assert_eq!(t.witnesses.len(), 3);
        t.witnesses.clear();
        t.unresolved = vec![
            UnresolvedRec {
                argument: NONE,
                provenance,
                relation: EdgeKind::Calls,
                src: 0,
                occurrence: 0,
                callee: 0,
                line: 1,
                reason: 0,
            };
            3
        ];
        assert!(
            validate_with(&writer::Encoder::new(&t), 2, 2, 100)
                .unwrap_err()
                .contains("UNRS")
        );
        assert_eq!(t.unresolved.len(), 3);
        t.unresolved.clear();
        t.edges = vec![edge; 3];
        assert!(
            validate_with(&writer::Encoder::new(&t), 4, 2, 100)
                .unwrap_err()
                .contains("EDGE")
        );
    }
}
