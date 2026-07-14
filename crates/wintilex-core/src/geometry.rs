//! Integer rectangles in virtual-screen coordinates.

use serde::{Deserialize, Serialize};

/// Axis-aligned rectangle. `x`/`y` are the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Which way a container is cut in two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Screen direction, used by focus / move / resize commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

impl Direction {
    pub fn axis(self) -> Axis {
        match self {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        }
    }

    /// `true` when the direction points towards increasing coordinates.
    pub fn is_positive(self) -> bool {
        matches!(self, Direction::Right | Direction::Down)
    }

    pub fn opposite(self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0, y: 0, width: 0, height: 0 };

    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { x: left, y: top, width: right - left, height: bottom - top }
    }

    pub const fn left(&self) -> i32 {
        self.x
    }

    pub const fn top(&self) -> i32 {
        self.y
    }

    pub const fn right(&self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.height
    }

    pub const fn area(&self) -> i64 {
        self.width as i64 * self.height as i64
    }

    pub const fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub const fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left() && x < self.right() && y >= self.top() && y < self.bottom()
    }

    /// Shrink on every side. Never collapses below 1x1.
    pub fn inset(&self, amount: i32) -> Rect {
        self.inset_xy(amount, amount)
    }

    pub fn inset_xy(&self, horizontal: i32, vertical: i32) -> Rect {
        Rect {
            x: self.x + horizontal,
            y: self.y + vertical,
            width: (self.width - horizontal * 2).max(1),
            height: (self.height - vertical * 2).max(1),
        }
    }

    pub fn intersection(&self, other: &Rect) -> Rect {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            Rect::ZERO
        } else {
            Rect::from_edges(left, top, right, bottom)
        }
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        !self.intersection(other).is_empty()
    }

    /// Split along `axis`, giving `ratio` of the space to the first half.
    pub fn split(&self, axis: Axis, ratio: f32) -> (Rect, Rect) {
        let ratio = ratio.clamp(0.05, 0.95);
        match axis {
            Axis::Horizontal => {
                let first = ((self.width as f32) * ratio).round() as i32;
                let first = first.clamp(1, (self.width - 1).max(1));
                (
                    Rect::new(self.x, self.y, first, self.height),
                    Rect::new(self.x + first, self.y, self.width - first, self.height),
                )
            }
            Axis::Vertical => {
                let first = ((self.height as f32) * ratio).round() as i32;
                let first = first.clamp(1, (self.height - 1).max(1));
                (
                    Rect::new(self.x, self.y, self.width, first),
                    Rect::new(self.x, self.y + first, self.width, self.height - first),
                )
            }
        }
    }

    /// The axis a container of this shape should be split on to stay close to
    /// square. Wide rectangles get cut vertically down the middle.
    pub fn preferred_split_axis(&self) -> Axis {
        if self.width >= self.height {
            Axis::Horizontal
        } else {
            Axis::Vertical
        }
    }

    /// Squared distance between the two centers. Used for nearest-window picks.
    pub fn center_distance_sq(&self, other: &Rect) -> i64 {
        let (ax, ay) = self.center();
        let (bx, by) = other.center();
        let dx = (ax - bx) as i64;
        let dy = (ay - by) as i64;
        dx * dx + dy * dy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_horizontal_halves() {
        let r = Rect::new(0, 0, 100, 50);
        let (a, b) = r.split(Axis::Horizontal, 0.5);
        assert_eq!(a, Rect::new(0, 0, 50, 50));
        assert_eq!(b, Rect::new(50, 0, 50, 50));
    }

    #[test]
    fn split_keeps_total_size() {
        let r = Rect::new(10, 20, 133, 77);
        for ratio in [0.1, 0.33, 0.5, 0.66, 0.9] {
            let (a, b) = r.split(Axis::Vertical, ratio);
            assert_eq!(a.height + b.height, r.height);
            assert_eq!(a.y, r.y);
            assert_eq!(b.bottom(), r.bottom());
        }
    }

    #[test]
    fn inset_never_collapses() {
        let r = Rect::new(0, 0, 4, 4);
        assert!(!r.inset(100).is_empty());
    }

    #[test]
    fn intersection_of_disjoint_is_empty() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(20, 20, 10, 10);
        assert!(a.intersection(&b).is_empty());
        assert!(!a.intersects(&b));
    }

    #[test]
    fn preferred_axis_follows_aspect() {
        assert_eq!(Rect::new(0, 0, 200, 100).preferred_split_axis(), Axis::Horizontal);
        assert_eq!(Rect::new(0, 0, 100, 200).preferred_split_axis(), Axis::Vertical);
    }
}
