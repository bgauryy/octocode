//! Cooperative cancellation shared by every tool that walks, parses, or reads.
//! The runtime's execution context implements it; tools poll it between units
//! of work and stop with the returned reason.
pub trait CancellationCheck: Sync {
    fn check(&self) -> Result<(), String>;
}
pub struct NeverCancel;
impl CancellationCheck for NeverCancel {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}
