use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct ByteRange {
    pub start_byte: u32,
    pub end_byte: u32,
}

impl ByteRange {
    pub const fn new(start_byte: u32, end_byte: u32) -> Self {
        Self {
            start_byte,
            end_byte,
        }
    }
    pub const fn len(self) -> u32 {
        self.end_byte.saturating_sub(self.start_byte)
    }
    pub const fn is_empty(self) -> bool {
        self.start_byte == self.end_byte
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct UnitHandle(pub u32);
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct RepresentationHandle(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ObservedSpan {
    pub unit: UnitHandle,
    pub range: ByteRange,
}
impl ObservedSpan {
    pub const fn root(start_byte: u32, end_byte: u32) -> Self {
        Self {
            unit: UnitHandle(0),
            range: ByteRange::new(start_byte, end_byte),
        }
    }
}
impl std::ops::Deref for ObservedSpan {
    type Target = ByteRange;
    fn deref(&self) -> &ByteRange {
        &self.range
    }
}
impl From<ByteRange> for ObservedSpan {
    fn from(range: ByteRange) -> Self {
        Self {
            unit: UnitHandle(0),
            range,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisOperation {
    StructuralSearch,
    SyntaxTree,
    SignatureOutline,
    Declarations,
    GraphFacts,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Producer {
    pub id: String,
    pub version: String,
}
impl Producer {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SourceSnapshot {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub byte_length: u32,
    pub encoding: Encoding,
    pub bom_bytes: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct TextRepresentation {
    pub id: String,
    pub source_digest: String,
    pub byte_length: u32,
    pub producer: Producer,
    pub source_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AnalysisUnit {
    pub id: String,
    pub adapter: Producer,
    pub operation: AnalysisOperation,
    pub representation_id: String,
    pub regions: Vec<ByteRange>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct SourceDescriptors {
    pub snapshots: Vec<SourceSnapshot>,
    pub representations: Vec<TextRepresentation>,
    pub units: Vec<AnalysisUnit>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum UnmappedReason {
    Synthetic,
    RawSourceUnavailable,
    MappingUnavailable,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct OriginalRange {
    pub source_id: String,
    pub raw_range: ByteRange,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum Contribution {
    Source(OriginalRange),
    Unmapped(UnmappedReason),
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum OriginBranch {
    Copied(OriginalRange),
    Transcoded(OriginalRange),
    Derived(Vec<Contribution>),
    Unmapped(UnmappedReason),
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum AnalysisMembership {
    Included,
    Excluded,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct OriginSegment {
    pub source_range: ByteRange,
    pub analysis: AnalysisMembership,
    pub branches: Vec<OriginBranch>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct OriginProjection {
    pub analysis_unit_id: String,
    pub segments: Vec<OriginSegment>,
}

pub(super) fn identity<T: Serialize>(value: &T) -> Result<String, super::SourceError> {
    use sha2::{Digest, Sha256};
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| super::SourceError::InvalidMap("identity encoding"))?;
    Ok(hex::encode(writer.0.finalize()))
}

impl OriginProjection {
    pub fn owned_bytes(&self) -> usize {
        self.analysis_unit_id.capacity()
            + self.segments.capacity() * std::mem::size_of::<OriginSegment>()
            + self
                .segments
                .iter()
                .map(|segment| {
                    segment.branches.capacity() * std::mem::size_of::<OriginBranch>()
                        + segment
                            .branches
                            .iter()
                            .map(|branch| match branch {
                                OriginBranch::Copied(value) | OriginBranch::Transcoded(value) => {
                                    value.source_id.capacity()
                                }
                                OriginBranch::Derived(values) => {
                                    values.capacity() * std::mem::size_of::<Contribution>()
                                        + values
                                            .iter()
                                            .map(|value| match value {
                                                Contribution::Source(value) => {
                                                    value.source_id.capacity()
                                                }
                                                Contribution::Unmapped(_) => 0,
                                            })
                                            .sum::<usize>()
                                }
                                OriginBranch::Unmapped(_) => 0,
                            })
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
}
