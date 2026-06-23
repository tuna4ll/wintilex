//! One large window plus a stack of the rest, in the style of dwm.

use crate::geometry::{Axis, Rect};
use crate::layout::{Arrangement, LayoutAlgorithm, LayoutContext, LayoutKind, SplitEdge, ROOT_KEY};

pub struct MainStack;

impl LayoutAlgorithm for MainStack {
    fn kind(&self) -> LayoutKind {
        LayoutKind::MainStack
    }

    fn arrange(&self, ctx: &LayoutContext) -> Arrangement {
        let area = ctx.area;
        if ctx.count == 1 {
            return Arrangement { tiles: vec![area], splits: Vec::new(), gap: 0 };
        }

        let axis = area.preferred_split_axis();
        let ratio = ctx.ratios.get(ROOT_KEY, ctx.options.main_ratio);
        let (first, second) = area.split(axis, ratio);
        let (main_area, stack_area) =
            if ctx.options.reversed { (second, first) } else { (first, second) };

        let mut tiles = Vec::with_capacity(ctx.count);
        tiles.push(main_area);

        // The stack runs across the short side of its own area.
        let stack_axis = match axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        let stack_count = (ctx.count - 1) as i32;
        let total = match stack_axis {
            Axis::Horizontal => stack_area.width,
            Axis::Vertical => stack_area.height,
        };
        let base = total / stack_count;
        let remainder = total % stack_count;
        let mut offset = 0;

        for index in 0..stack_count {
            let size = base + if index < remainder { 1 } else { 0 };
            tiles.push(match stack_axis {
                Axis::Horizontal => {
                    Rect::new(stack_area.x + offset, stack_area.y, size, stack_area.height)
                }
                Axis::Vertical => {
                    Rect::new(stack_area.x, stack_area.y + offset, stack_area.width, size)
                }
            });
            offset += size;
        }

        let splits = vec![SplitEdge {
            key: ROOT_KEY,
            axis,
            position: match axis {
                Axis::Horizontal => first.right(),
                Axis::Vertical => first.bottom(),
            },
            container: area,
            first: if ctx.options.reversed { (1, ctx.count) } else { (0, 1) },
            second: if ctx.options.reversed { (0, 1) } else { (1, ctx.count) },
        }];

        Arrangement { tiles, splits, gap: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{arrange, LayoutOptions, Ratios};

    fn plain() -> LayoutOptions {
        LayoutOptions { gap: 0, outer_gap: 0, main_ratio: 0.6, reversed: false }
    }

    #[test]
    fn main_takes_the_configured_share() {
        let result = arrange(&MainStack, Rect::new(0, 0, 1000, 800), 3, &plain(), &Ratios::new());
        assert_eq!(result.tiles[0], Rect::new(0, 0, 600, 800));
        assert_eq!(result.tiles[1], Rect::new(600, 0, 400, 400));
        assert_eq!(result.tiles[2], Rect::new(600, 400, 400, 400));
    }

    #[test]
    fn reversed_puts_main_on_the_right() {
        let options = LayoutOptions { reversed: true, ..plain() };
        let result = arrange(&MainStack, Rect::new(0, 0, 1000, 800), 2, &options, &Ratios::new());
        assert_eq!(result.tiles[0].x, 600);
        assert_eq!(result.tiles[1].x, 0);
    }
}
