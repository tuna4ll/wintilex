//! Layout algorithms.
//!
//! A layout is a pure function: given an area and a window count it returns one
//! rect per window. Alongside the rects it reports the *split edges* it used,
//! which is what lets a manual resize be folded back into the layout instead of
//! being undone on the next pass.

mod bsp;
mod columns;
mod main_stack;
mod monocle;

pub use bsp::Bsp;
pub use columns::{Columns, Rows};
pub use main_stack::MainStack;
pub use monocle::Monocle;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::geometry::{Axis, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutKind {
    Bsp,
    Columns,
    Rows,
    MainStack,
    Monocle,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 5] = [
        LayoutKind::Bsp,
        LayoutKind::Columns,
        LayoutKind::Rows,
        LayoutKind::MainStack,
        LayoutKind::Monocle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LayoutKind::Bsp => "Binary split",
            LayoutKind::Columns => "Columns",
            LayoutKind::Rows => "Rows",
            LayoutKind::MainStack => "Main and stack",
            LayoutKind::Monocle => "Monocle",
        }
    }

    pub fn build(self) -> Box<dyn LayoutAlgorithm> {
        match self {
            LayoutKind::Bsp => Box::new(Bsp),
            LayoutKind::Columns => Box::new(Columns),
            LayoutKind::Rows => Box::new(Rows),
            LayoutKind::MainStack => Box::new(MainStack),
            LayoutKind::Monocle => Box::new(Monocle),
        }
    }

    pub fn next(self) -> LayoutKind {
        let all = Self::ALL;
        let index = all.iter().position(|k| *k == self).unwrap_or(0);
        all[(index + 1) % all.len()]
    }
}

impl Default for LayoutKind {
    fn default() -> Self {
        LayoutKind::Bsp
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LayoutOptions {
    /// Space between two neighbouring windows.
    pub gap: i32,
    /// Space between the outermost windows and the screen edge.
    pub outer_gap: i32,
    /// Share of the screen the main area takes in `MainStack`.
    pub main_ratio: f32,
    /// Mirror the layout, putting the main area on the right or bottom.
    pub reversed: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self { gap: 8, outer_gap: 8, main_ratio: 0.6, reversed: false }
    }
}

/// A boundary between two groups of tiles that the user can drag.
///
/// `key` addresses the stored ratio; `container` is the rect that was cut and
/// `position` is where the cut landed in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitEdge {
    pub key: RatioKey,
    pub axis: Axis,
    pub position: i32,
    pub container: Rect,
    /// Half-open index range of the tiles before the cut.
    pub first: (usize, usize),
    /// Half-open index range of the tiles after the cut.
    pub second: (usize, usize),
}

impl SplitEdge {
    pub fn contains_first(&self, index: usize) -> bool {
        index >= self.first.0 && index < self.first.1
    }

    pub fn contains_second(&self, index: usize) -> bool {
        index >= self.second.0 && index < self.second.1
    }

    /// Turn a screen coordinate on this edge back into a ratio.
    pub fn ratio_at(&self, position: i32) -> f32 {
        let (start, size) = match self.axis {
            Axis::Horizontal => (self.container.left(), self.container.width),
            Axis::Vertical => (self.container.top(), self.container.height),
        };
        if size <= 0 {
            return 0.5;
        }
        ((position - start) as f32 / size as f32).clamp(MIN_RATIO, MAX_RATIO)
    }
}

/// Identifier of one adjustable split, addressed like a binary heap: the root
/// is 1, its two halves are 2 and 3, and so on down the tree.
pub type RatioKey = u32;

pub const ROOT_KEY: RatioKey = 1;
pub const MIN_RATIO: f32 = 0.1;
pub const MAX_RATIO: f32 = 0.9;

/// Ratios the user has dragged away from their default.
///
/// Anything not in the map falls back to the layout's own idea of a balanced
/// split, so a fresh workspace needs no state at all.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ratios {
    overrides: HashMap<RatioKey, f32>,
}

impl Ratios {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: RatioKey, fallback: f32) -> f32 {
        self.overrides.get(&key).copied().unwrap_or(fallback).clamp(MIN_RATIO, MAX_RATIO)
    }

    pub fn set(&mut self, key: RatioKey, ratio: f32) {
        self.overrides.insert(key, ratio.clamp(MIN_RATIO, MAX_RATIO));
    }

    /// Nudge a split by a fraction of its container. Used by the resize hotkeys.
    pub fn nudge(&mut self, key: RatioKey, fallback: f32, delta: f32) {
        let current = self.get(key, fallback);
        self.set(key, current + delta);
    }

    pub fn clear(&mut self) {
        self.overrides.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    pub fn len(&self) -> usize {
        self.overrides.len()
    }
}

/// Child keys of a split, mirroring the heap addressing described above.
pub fn child_keys(key: RatioKey) -> (RatioKey, RatioKey) {
    // Stop growing once the key would overflow; very deep trees simply share
    // the parent ratio, which is invisible in practice.
    if key > RatioKey::MAX / 2 - 1 {
        (key, key)
    } else {
        (key * 2, key * 2 + 1)
    }
}

/// Result of running a layout.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Arrangement {
    /// One rect per window, in the same order the windows were given.
    pub tiles: Vec<Rect>,
    /// Boundaries between tiles, in no particular order.
    pub splits: Vec<SplitEdge>,
}

impl Arrangement {
    /// Apply the inner gap. Each tile shrinks by half a gap on every side, so
    /// the visible space between two neighbours adds up to one full gap.
    pub fn with_gaps(mut self, gap: i32) -> Self {
        if gap > 0 {
            let half = gap / 2;
            for tile in &mut self.tiles {
                *tile = tile.inset(half);
            }
        }
        self
    }

    /// The split whose boundary matches `position` on `axis` and that has
    /// `index` on the side the drag came from.
    pub fn find_split(
        &self,
        index: usize,
        axis: Axis,
        position: i32,
        from_first_side: bool,
        tolerance: i32,
    ) -> Option<&SplitEdge> {
        self.splits
            .iter()
            .filter(|split| split.axis == axis)
            .filter(|split| (split.position - position).abs() <= tolerance)
            .find(|split| {
                if from_first_side {
                    split.contains_first(index)
                } else {
                    split.contains_second(index)
                }
            })
    }
}

/// Everything a layout needs in order to place windows.
pub struct LayoutContext<'a> {
    /// The usable area, already reduced by the outer gap.
    pub area: Rect,
    pub count: usize,
    pub options: &'a LayoutOptions,
    pub ratios: &'a Ratios,
}

pub trait LayoutAlgorithm: Send + Sync {
    fn kind(&self) -> LayoutKind;

    fn arrange(&self, ctx: &LayoutContext) -> Arrangement;
}

/// Run a layout end to end: outer gap, arrangement, inner gaps.
pub fn arrange(
    algorithm: &dyn LayoutAlgorithm,
    area: Rect,
    count: usize,
    options: &LayoutOptions,
    ratios: &Ratios,
) -> Arrangement {
    if count == 0 || area.is_empty() {
        return Arrangement::default();
    }
    let area = area.inset(options.outer_gap.max(0));
    let ctx = LayoutContext { area, count, options, ratios };
    algorithm.arrange(&ctx).with_gaps(options.gap.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> LayoutOptions {
        LayoutOptions { gap: 0, outer_gap: 0, ..Default::default() }
    }

    #[test]
    fn every_layout_fills_the_area_without_overlap() {
        let area = Rect::new(0, 0, 1920, 1080);
        let ratios = Ratios::new();
        for kind in LayoutKind::ALL {
            if kind == LayoutKind::Monocle {
                continue; // monocle stacks on purpose
            }
            let algorithm = kind.build();
            for count in 1..=8 {
                let result = arrange(algorithm.as_ref(), area, count, &options(), &ratios);
                assert_eq!(result.tiles.len(), count, "{kind:?} with {count}");

                let covered: i64 = result.tiles.iter().map(|t| t.area()).sum();
                assert_eq!(covered, area.area(), "{kind:?} with {count} leaves holes");

                for i in 0..count {
                    for j in (i + 1)..count {
                        assert!(
                            !result.tiles[i].intersects(&result.tiles[j]),
                            "{kind:?} overlaps at {i}/{j} with {count}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn single_window_gets_everything() {
        let area = Rect::new(10, 20, 800, 600);
        let ratios = Ratios::new();
        for kind in LayoutKind::ALL {
            let result = arrange(kind.build().as_ref(), area, 1, &options(), &ratios);
            assert_eq!(result.tiles[0], area, "{kind:?}");
        }
    }

    #[test]
    fn gaps_shrink_every_tile() {
        let area = Rect::new(0, 0, 1000, 1000);
        let opts = LayoutOptions { gap: 10, outer_gap: 20, ..Default::default() };
        let result = arrange(&Bsp, area, 2, &opts, &Ratios::new());
        assert_eq!(result.tiles[0].x, 25);
        assert_eq!(result.tiles[0].y, 25);
        assert_eq!(result.tiles[1].right(), 975);
    }

    #[test]
    fn ratio_overrides_are_clamped() {
        let mut ratios = Ratios::new();
        ratios.set(ROOT_KEY, 5.0);
        assert_eq!(ratios.get(ROOT_KEY, 0.5), MAX_RATIO);
        ratios.set(ROOT_KEY, -1.0);
        assert_eq!(ratios.get(ROOT_KEY, 0.5), MIN_RATIO);
    }

    #[test]
    fn split_edges_are_reported_for_each_boundary() {
        let area = Rect::new(0, 0, 1000, 800);
        let result = arrange(&Bsp, area, 4, &options(), &Ratios::new());
        // A four-way split has one root cut plus one inside each half.
        assert_eq!(result.splits.len(), 3);
        assert!(result.splits.iter().any(|s| s.key == ROOT_KEY));
    }
}
