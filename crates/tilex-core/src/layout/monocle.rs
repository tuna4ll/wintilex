//! Every window fills the whole area; only the focused one is visible.

use crate::layout::{Arrangement, LayoutAlgorithm, LayoutContext, LayoutKind};

pub struct Monocle;

impl LayoutAlgorithm for Monocle {
    fn kind(&self) -> LayoutKind {
        LayoutKind::Monocle
    }

    fn arrange(&self, ctx: &LayoutContext) -> Arrangement {
        Arrangement { tiles: vec![ctx.area; ctx.count], splits: Vec::new() }
    }
}
