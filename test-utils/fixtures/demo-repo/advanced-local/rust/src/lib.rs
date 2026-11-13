pub fn target() {}

#[cfg(feature = "base")]
mod base_feature;

#[cfg(feature = "bright")]
mod bright_feature;

#[cfg(feature = "dim")]
mod dim_feature;

#[cfg(transport_cfg)]
mod custom_cfg;

mod always;
