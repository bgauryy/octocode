use super::{
    AnalysisOperation, ByteRange, Producer, RepresentationHandle, SourceError, UnitHandle,
    UnmappedReason,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum MapBranch {
    Exact {
        input: RepresentationHandle,
        range: ByteRange,
    },
    Derived {
        contributors: Vec<MapContribution>,
    },
    Unmapped(UnmappedReason),
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum MapContribution {
    Source {
        input: RepresentationHandle,
        range: ByteRange,
    },
    Unmapped(UnmappedReason),
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct MapSegment {
    pub range: ByteRange,
    pub branches: Vec<MapBranch>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum RepresentationContent {
    Root {
        source_index: u32,
        source_digest: String,
        byte_length: u32,
    },
    Prefix {
        input: RepresentationHandle,
        end_byte: u32,
        tail: String,
    },
    Mapped {
        text: String,
        segments: Vec<MapSegment>,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct RepresentationTemplate {
    pub producer: Producer,
    pub content: RepresentationContent,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct UnitTemplate {
    pub adapter: Producer,
    pub operation: AnalysisOperation,
    pub representation: RepresentationHandle,
    pub regions: Vec<ByteRange>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct SourceTemplates {
    pub representations: Vec<RepresentationTemplate>,
    pub units: Vec<UnitTemplate>,
}
impl SourceTemplates {
    /// SHA-256 of the text the first root representation was built from.
    pub fn root_digest(&self) -> Option<&str> {
        self.representations
            .iter()
            .find_map(|representation| match &representation.content {
                RepresentationContent::Root { source_digest, .. } => Some(source_digest.as_str()),
                _ => None,
            })
    }
    pub fn new(
        text: &str,
        adapter: Producer,
        operation: AnalysisOperation,
    ) -> Result<Self, SourceError> {
        let mut result = Self::default();
        let representation = result.add_root(text, 0)?;
        result.add_unit(
            adapter,
            operation,
            representation,
            vec![ByteRange::new(
                0,
                u32::try_from(text.len()).map_err(|_| SourceError::LimitExceeded("text length"))?,
            )],
        )?;
        Ok(result)
    }
    pub fn add_root(
        &mut self,
        text: &str,
        source_index: u32,
    ) -> Result<RepresentationHandle, SourceError> {
        let byte_length =
            u32::try_from(text.len()).map_err(|_| SourceError::LimitExceeded("text length"))?;
        self.add_representation(
            Producer::new("unicode-source", "1"),
            RepresentationContent::Root {
                source_index,
                source_digest: crate::digest::sha256(text.as_bytes()),
                byte_length,
            },
        )
    }
    pub fn add_representation(
        &mut self,
        producer: Producer,
        content: RepresentationContent,
    ) -> Result<RepresentationHandle, SourceError> {
        if let RepresentationContent::Mapped { text, .. } = &content {
            u32::try_from(text.len())
                .map_err(|_| SourceError::LimitExceeded("representation bytes"))?;
        }
        let handle = RepresentationHandle(
            u32::try_from(self.representations.len())
                .map_err(|_| SourceError::LimitExceeded("representation count"))?,
        );
        self.representations
            .push(RepresentationTemplate { producer, content });
        Ok(handle)
    }
    pub fn add_unit(
        &mut self,
        adapter: Producer,
        operation: AnalysisOperation,
        representation: RepresentationHandle,
        regions: Vec<ByteRange>,
    ) -> Result<UnitHandle, SourceError> {
        let handle = UnitHandle(
            u32::try_from(self.units.len())
                .map_err(|_| SourceError::LimitExceeded("unit count"))?,
        );
        self.units.push(UnitTemplate {
            adapter,
            operation,
            representation,
            regions,
        });
        Ok(handle)
    }
    pub fn bind<'a>(
        &self,
        source: &'a super::AdmittedSource<'a>,
        budget: &mut super::MapBudget<'_>,
    ) -> Result<super::BoundSource<'a>, SourceError> {
        self.bind_sources(&[source], budget)
    }
    pub fn bind_sources<'a>(
        &self,
        sources: &[&'a super::AdmittedSource<'a>],
        budget: &mut super::MapBudget<'_>,
    ) -> Result<super::BoundSource<'a>, SourceError> {
        budget.allocate(self.heap_bytes())?;
        super::BoundSource::new(std::sync::Arc::new(self.clone()), sources, budget)
    }
    pub fn bind_shared<'a>(
        self: &std::sync::Arc<Self>,
        source: &'a super::AdmittedSource<'a>,
        budget: &mut super::MapBudget<'_>,
    ) -> Result<super::BoundSource<'a>, SourceError> {
        super::BoundSource::new(std::sync::Arc::clone(self), &[source], budget)
    }
    pub fn heap_bytes(&self) -> usize {
        let producer = |p: &Producer| p.id.capacity() + p.version.capacity();
        self.representations.capacity() * std::mem::size_of::<RepresentationTemplate>()
            + self.units.capacity() * std::mem::size_of::<UnitTemplate>()
            + self
                .units
                .iter()
                .map(|u| {
                    producer(&u.adapter) + u.regions.capacity() * std::mem::size_of::<ByteRange>()
                })
                .sum::<usize>()
            + self
                .representations
                .iter()
                .map(|r| {
                    producer(&r.producer)
                        + match &r.content {
                            RepresentationContent::Root { source_digest, .. } => {
                                source_digest.capacity()
                            }
                            RepresentationContent::Prefix { tail, .. } => tail.capacity(),
                            RepresentationContent::Mapped { text, segments } => {
                                text.capacity()
                                    + segments.capacity() * std::mem::size_of::<MapSegment>()
                                    + segments
                                        .iter()
                                        .map(|s| {
                                            s.branches.capacity() * std::mem::size_of::<MapBranch>()
                                            + s.branches
                                                .iter()
                                                .map(|b| match b {
                                                    MapBranch::Derived { contributors } => {
                                                        contributors.capacity()
                                                            * std::mem::size_of::<MapContribution>()
                                                    }
                                                    _ => 0,
                                                })
                                                .sum::<usize>()
                                        })
                                        .sum::<usize>()
                            }
                        }
                })
                .sum::<usize>()
    }
}
