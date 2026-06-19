//! Even columns and rows.

use crate::geometry::{Axis, Rect};
use crate::layout::{Arrangement, LayoutAlgorithm, LayoutContext, LayoutKind, SplitEdge, ROOT_KEY};

/// Windows side by side, each taking a full-height column.
pub struct Columns;

/// Windows stacked, each taking a full-width row.
pub struct Rows;

impl LayoutAlgorithm for Columns {
    fn kind(&self) -> LayoutKind {
        LayoutKind::Columns
    }

    fn arrange(&self, ctx: &LayoutContext) -> Arrangement {
        stripe(ctx, Axis::Horizontal)
    }
}

impl LayoutAlgorithm for Rows {
    fn kind(&self) -> LayoutKind {
        LayoutKind::Rows
    }

    fn arrange(&self, ctx: &LayoutContext) -> Arrangement {
        stripe(ctx, Axis::Vertical)
    }
}

/// Cut `area` into `count` equal strips along `axis`.
///
/// The remainder is spread over the leading strips so the strips together
/// always cover the area exactly, with no off-by-one seam on the last one.
fn stripe(ctx: &LayoutContext, axis: Axis) -> Arrangement {
    let area = ctx.area;
    let count = ctx.count as i32;
    let total = match axis {
        Axis::Horizontal => area.width,
        Axis::Vertical => area.height,
    };

    let base = total / count;
    let remainder = total % count;

    let mut tiles = Vec::with_capacity(ctx.count);
    let mut splits = Vec::with_capacity(ctx.count.saturating_sub(1));
    let mut offset = 0;

    for index in 0..ctx.count {
        let extra = if (index as i32) < remainder { 1 } else { 0 };
        let size = base + extra;
        let tile = match axis {
            Axis::Horizontal => Rect::new(area.x + offset, area.y, size, area.height),
            Axis::Vertical => Rect::new(area.x, area.y + offset, area.width, size),
        };
        offset += size;

        if index + 1 < ctx.count {
            splits.push(SplitEdge {
                // Strips are evenly spaced by definition, so every boundary
                // shares one key and dragging one of them is not persisted.
                key: ROOT_KEY,
                axis,
                position: match axis {
                    Axis::Horizontal => tile.right(),
                    Axis::Vertical => tile.bottom(),
                },
                container: area,
                first: (index, index + 1),
                second: (index + 1, index + 2),
            });
        }

        tiles.push(tile);
    }

    Arrangement { tiles, splits }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{arrange, LayoutOptions, Ratios};

    fn plain() -> LayoutOptions {
        LayoutOptions { gap: 0, outer_gap: 0, ..Default::default() }
    }

    #[test]
    fn columns_are_even() {
        let result = arrange(&Columns, Rect::new(0, 0, 900, 600), 3, &plain(), &Ratios::new());
        for (index, tile) in result.tiles.iter().enumerate() {
            assert_eq!(tile.width, 300);
            assert_eq!(tile.x, index as i32 * 300);
            assert_eq!(tile.height, 600);
        }
    }

    #[test]
    fn remainder_is_spread_over_the_first_strips() {
        let result = arrange(&Rows, Rect::new(0, 0, 100, 1000), 3, &plain(), &Ratios::new());
        assert_eq!(result.tiles[0].height, 334);
        assert_eq!(result.tiles[1].height, 333);
        assert_eq!(result.tiles[2].height, 333);
        assert_eq!(result.tiles[2].bottom(), 1000);
    }
}
