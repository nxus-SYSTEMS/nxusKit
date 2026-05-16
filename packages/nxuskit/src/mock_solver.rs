//! Public CE mock solver placeholder.
//!
//! Solver test doubles for Pro APIs are not shipped in public CE source.

#[derive(Debug, Clone, Default)]
pub struct MockSolverProvider;

#[derive(Debug, Clone, Default)]
pub struct MockSolverProviderBuilder;

impl MockSolverProviderBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build(self) -> MockSolverProvider {
        MockSolverProvider
    }
}
