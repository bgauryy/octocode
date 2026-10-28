use super::*;

fn budget() -> MapBudget<'static> {
    MapBudget::new(
        MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        &|| false,
    )
}

fn mapped(text: &str, segments: Vec<MapSegment>) -> SourceTemplates {
    let mut templates = SourceTemplates::new(
        text,
        Producer::new("fixture", "1"),
        AnalysisOperation::GraphFacts,
    )
    .unwrap();
    let representation = templates
        .add_representation(
            Producer::new("fixture-map", "1"),
            RepresentationContent::Mapped {
                text: text.into(),
                segments,
            },
        )
        .unwrap();
    templates
        .add_unit(
            Producer::new("fixture-map", "1"),
            AnalysisOperation::StructuralSearch,
            representation,
            vec![ByteRange::new(0, text.len() as u32)],
        )
        .unwrap();
    templates
}

mod composition;
mod decoding;
mod profiles;
mod validation;
