mod algorithms;
mod arguments;
mod diagnostics;
mod evidence;
mod model;
mod test_identity;
pub(crate) use arguments::argument_occurrence_id;
pub use arguments::{
    ARGUMENT_DERIVATION, GraphArgumentBinding, GraphArgumentForm, GraphArgumentGroup,
    GraphArgumentGroupObservation, GraphArgumentObservation, GraphArgumentSlot,
    GraphArgumentValueKind, GraphFactArgumentSite,
};
pub use diagnostics::{GraphFactCode, GraphFactDiagnostic};
pub use test_identity::{GraphTestIdentity, GraphTestKind};

pub use evidence::{
    GraphAssessmentMissingReason, GraphParseAssessment, GraphParseScope, GraphRecoveryReason,
};

pub use algorithms::{
    Condensed, FileGraphNode, condense, scc, scc_filtered, strongly_connected_components,
};
pub use model::{
    COMMONJS_WHOLE_EXPORT, GraphCallBinding, GraphCallBoundary, GraphFactCall, GraphFactCommonJs,
    GraphFactCommonJsBinding, GraphFactDeclaration, GraphFactEdge, GraphFactExport,
    GraphFactImport, GraphFactPath, GraphFactRustModule, GraphFactsDocument, GraphFactsTypedEntry,
    GraphPosition, GraphRange, GraphReceiverCall, GraphReferenceCount,
};

pub use crate::text::analysis::{ByteRange, ObservedSpan};
