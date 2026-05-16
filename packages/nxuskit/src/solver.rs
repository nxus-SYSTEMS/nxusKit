//! Public CE solver wrapper stub.

use crate::{NxuskitError, solver_types::SolverStreamChunk};

pub struct SolverStreamReceiver;

impl Iterator for SolverStreamReceiver {
    type Item = Result<SolverStreamChunk, NxuskitError>;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
