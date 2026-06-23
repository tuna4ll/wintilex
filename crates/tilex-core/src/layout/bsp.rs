//! Binary space partitioning.
//!
//! The area is cut in half along its longer side, the windows are dealt out to
//! the two halves and each half is cut again until every window has a tile.
//! Splitting the count evenly and weighting the cut by that count keeps every
//! window at roughly the same area, which is what "balanced" means here:
//!
//! ```text
//! 1 window        2 windows        3 windows        4 windows
//! +---------+     +----+----+      +------+--+      +----+----+
//! |         |     |    |    |      |      |  |      |    |    |
//! |    A    |     | A  | B  |      |  A   |  |      | A  | B  |
//! |         |     |    |    |      +------+C |      +----+----+
//! |         |     |    |    |      |  B   |  |      | C  | D  |
//! +---------+     +----+----+      +------+--+      +----+----+
//! ```

use crate::geometry::Rect;
use crate::layout::{
    child_keys, Arrangement, LayoutAlgorithm, LayoutContext, LayoutKind, RatioKey, SplitEdge,
    ROOT_KEY,
};

pub struct Bsp;

impl LayoutAlgorithm for Bsp {
    fn kind(&self) -> LayoutKind {
        LayoutKind::Bsp
    }

    fn arrange(&self, ctx: &LayoutContext) -> Arrangement {
        let mut result = Arrangement {
            tiles: vec![Rect::ZERO; ctx.count],
            splits: Vec::with_capacity(ctx.count.saturating_sub(1)),
            gap: 0,
        };
        split(ctx, ctx.area, 0, ctx.count, ROOT_KEY, &mut result);
        result
    }
}

/// Place windows `[start, end)` inside `area`.
fn split(
    ctx: &LayoutContext,
    area: Rect,
    start: usize,
    end: usize,
    key: RatioKey,
    result: &mut Arrangement,
) {
    let count = end - start;
    if count == 0 {
        return;
    }
    if count == 1 {
        result.tiles[start] = area;
        return;
    }

    let first_count = count.div_ceil(2);
    let middle = start + first_count;

    // Weighting the cut by the window count is what keeps tiles equal-sized
    // when the count is odd.
    let balanced = first_count as f32 / count as f32;
    let ratio = ctx.ratios.get(key, balanced);

    let axis = area.preferred_split_axis();
    let (first_area, second_area) = area.split(axis, ratio);

    result.splits.push(SplitEdge {
        key,
        axis,
        position: match axis {
            crate::geometry::Axis::Horizontal => first_area.right(),
            crate::geometry::Axis::Vertical => first_area.bottom(),
        },
        container: area,
        first: (start, middle),
        second: (middle, end),
    });

    let (first_key, second_key) = child_keys(key);
    split(ctx, first_area, start, middle, first_key, result);
    split(ctx, second_area, middle, end, second_key, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{arrange, LayoutOptions, Ratios};

    fn plain() -> LayoutOptions {
        LayoutOptions { gap: 0, outer_gap: 0, ..Default::default() }
    }

    #[test]
    fn two_windows_split_the_long_side() {
        let area = Rect::new(0, 0, 1920, 1080);
        let result = arrange(&Bsp, area, 2, &plain(), &Ratios::new());
        assert_eq!(result.tiles[0], Rect::new(0, 0, 960, 1080));
        assert_eq!(result.tiles[1], Rect::new(960, 0, 960, 1080));
    }

    #[test]
    fn tall_area_splits_horizontally() {
        let area = Rect::new(0, 0, 800, 1600);
        let result = arrange(&Bsp, area, 2, &plain(), &Ratios::new());
        assert_eq!(result.tiles[0], Rect::new(0, 0, 800, 800));
        assert_eq!(result.tiles[1], Rect::new(0, 800, 800, 800));
    }

    #[test]
    fn three_windows_end_up_roughly_equal() {
        let area = Rect::new(0, 0, 1200, 900);
        let result = arrange(&Bsp, area, 3, &plain(), &Ratios::new());
        let target = area.area() / 3;
        for tile in &result.tiles {
            let deviation = (tile.area() - target).abs();
            assert!(deviation * 10 < target, "tile {tile:?} is far from balanced");
        }
    }

    #[test]
    fn ratio_override_moves_the_root_split() {
        let area = Rect::new(0, 0, 1000, 500);
        let mut ratios = Ratios::new();
        ratios.set(ROOT_KEY, 0.7);
        let result = arrange(&Bsp, area, 2, &plain(), &ratios);
        assert_eq!(result.tiles[0].width, 700);
        assert_eq!(result.tiles[1].width, 300);
    }

    #[test]
    fn many_windows_stay_inside_the_area() {
        let area = Rect::new(100, 50, 1920, 1080);
        let result = arrange(&Bsp, area, 12, &plain(), &Ratios::new());
        for tile in &result.tiles {
            assert_eq!(tile.intersection(&area), *tile);
            assert!(!tile.is_empty());
        }
    }
}
