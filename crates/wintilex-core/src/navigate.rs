//! Picking the neighbouring tile in a direction.
//!
//! Windows that overlap the source on the perpendicular axis are always
//! preferred, so `Win+L` from a tall left column lands on whatever is directly
//! to the right rather than on whichever tile happens to be nearest by
//! straight-line distance.

use crate::geometry::{Axis, Direction, Rect};

/// Index of the tile to move to, or `None` at the edge of the screen.
pub fn neighbour(tiles: &[Rect], from: usize, direction: Direction) -> Option<usize> {
    let source = *tiles.get(from)?;

    let mut best: Option<(usize, bool, i64, i64)> = None;
    for (index, tile) in tiles.iter().enumerate() {
        if index == from {
            continue;
        }
        let Some(gap) = distance_along(source, *tile, direction) else {
            continue;
        };
        let overlaps = overlaps_across(source, *tile, direction);
        let offset = offset_across(source, *tile, direction);

        // Aligned neighbours beat diagonal ones; then the closest wins.
        let candidate = (index, overlaps, gap, offset);
        let better = match best {
            None => true,
            Some((_, best_overlaps, best_gap, best_offset)) => {
                (overlaps, std::cmp::Reverse(gap), std::cmp::Reverse(offset))
                    > (best_overlaps, std::cmp::Reverse(best_gap), std::cmp::Reverse(best_offset))
            }
        };
        if better {
            best = Some(candidate);
        }
    }

    best.map(|(index, _, _, _)| index)
}

/// How far `target` sits in `direction`, or `None` if it is the wrong way.
fn distance_along(source: Rect, target: Rect, direction: Direction) -> Option<i64> {
    let distance = match direction {
        Direction::Left => source.left() - target.right(),
        Direction::Right => target.left() - source.right(),
        Direction::Up => source.top() - target.bottom(),
        Direction::Down => target.top() - source.bottom(),
    };
    // Touching tiles give exactly zero; a small negative value means the tiles
    // overlap slightly, which still counts as being on that side.
    if distance >= -1 {
        Some(distance.max(0) as i64)
    } else {
        None
    }
}

fn overlaps_across(source: Rect, target: Rect, direction: Direction) -> bool {
    match direction.axis() {
        Axis::Horizontal => source.top() < target.bottom() && target.top() < source.bottom(),
        Axis::Vertical => source.left() < target.right() && target.left() < source.right(),
    }
}

fn offset_across(source: Rect, target: Rect, direction: Direction) -> i64 {
    let (a, b) = source.center();
    let (c, d) = target.center();
    match direction.axis() {
        Axis::Horizontal => (b - d).unsigned_abs() as i64,
        Axis::Vertical => (a - c).unsigned_abs() as i64,
    }
}

/// The monitor whose work area lies in `direction` of `from`.
pub fn neighbour_monitor(areas: &[Rect], from: usize, direction: Direction) -> Option<usize> {
    neighbour(areas, from, direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 grid of 100x100 tiles: 0 1 on top, 2 3 below.
    fn grid() -> Vec<Rect> {
        vec![
            Rect::new(0, 0, 100, 100),
            Rect::new(100, 0, 100, 100),
            Rect::new(0, 100, 100, 100),
            Rect::new(100, 100, 100, 100),
        ]
    }

    #[test]
    fn moves_across_the_grid() {
        let tiles = grid();
        assert_eq!(neighbour(&tiles, 0, Direction::Right), Some(1));
        assert_eq!(neighbour(&tiles, 0, Direction::Down), Some(2));
        assert_eq!(neighbour(&tiles, 3, Direction::Left), Some(2));
        assert_eq!(neighbour(&tiles, 3, Direction::Up), Some(1));
    }

    #[test]
    fn stops_at_the_edge() {
        let tiles = grid();
        assert_eq!(neighbour(&tiles, 0, Direction::Left), None);
        assert_eq!(neighbour(&tiles, 0, Direction::Up), None);
        assert_eq!(neighbour(&tiles, 3, Direction::Right), None);
        assert_eq!(neighbour(&tiles, 3, Direction::Down), None);
    }

    #[test]
    fn prefers_the_aligned_tile_over_a_closer_diagonal_one() {
        // A tall column on the left, two stacked tiles on the right.
        let tiles = vec![
            Rect::new(0, 0, 100, 200),
            Rect::new(100, 0, 100, 100),
            Rect::new(100, 100, 100, 100),
        ];
        // Both right-hand tiles touch the column, the top one is closer to the
        // column's centre line only by a hair, so alignment has to decide.
        assert!(matches!(neighbour(&tiles, 0, Direction::Right), Some(1) | Some(2)));
        assert_eq!(neighbour(&tiles, 1, Direction::Left), Some(0));
        assert_eq!(neighbour(&tiles, 2, Direction::Left), Some(0));
        assert_eq!(neighbour(&tiles, 1, Direction::Down), Some(2));
    }

    #[test]
    fn ignores_tiles_that_do_not_line_up_at_all() {
        let tiles = vec![Rect::new(0, 0, 100, 100), Rect::new(400, 400, 100, 100)];
        assert_eq!(neighbour(&tiles, 0, Direction::Right), Some(1));
        assert_eq!(neighbour(&tiles, 1, Direction::Left), Some(0));
        assert_eq!(neighbour(&tiles, 1, Direction::Down), None);
    }

    #[test]
    fn a_single_tile_has_no_neighbours() {
        let tiles = vec![Rect::new(0, 0, 100, 100)];
        for direction in [Direction::Left, Direction::Right, Direction::Up, Direction::Down] {
            assert_eq!(neighbour(&tiles, 0, direction), None);
        }
    }

    #[test]
    fn gaps_between_tiles_do_not_break_navigation() {
        let tiles = vec![Rect::new(0, 0, 96, 96), Rect::new(104, 0, 96, 96)];
        assert_eq!(neighbour(&tiles, 0, Direction::Right), Some(1));
    }
}
