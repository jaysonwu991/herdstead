//! Rectangle geometry shared by the layout engine and the renderers.

use serde::{Deserialize, Serialize};

/// A cell rectangle. `x`/`y` are absolute screen columns/rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn area(self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    pub const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }

    /// The inner area after a one-cell frame, clamped to nothing.
    pub fn inner(self) -> Self {
        if self.width <= 2 || self.height <= 2 {
            return Self::new(self.x, self.y, 0, 0);
        }
        Self::new(self.x + 1, self.y + 1, self.width - 2, self.height - 2)
    }

    pub fn contains(self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn center(self) -> (i32, i32) {
        (
            i32::from(self.x) + i32::from(self.width) / 2,
            i32::from(self.y) + i32::from(self.height) / 2,
        )
    }

    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }

    pub fn shrink_top(self, rows: u16) -> Self {
        let rows = rows.min(self.height);
        Self::new(self.x, self.y + rows, self.width, self.height - rows)
    }

    pub fn shrink_bottom(self, rows: u16) -> Self {
        let rows = rows.min(self.height);
        Self::new(self.x, self.y, self.width, self.height - rows)
    }

    pub fn shrink_right(self, cols: u16) -> Self {
        let cols = cols.min(self.width);
        Self::new(self.x, self.y, self.width - cols, self.height)
    }

    pub fn shrink_left(self, cols: u16) -> Self {
        let cols = cols.min(self.width);
        Self::new(self.x + cols, self.y, self.width - cols, self.height)
    }
}

/// Directions used for focus movement, resizing and splitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub const fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }

    pub const fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Up | Self::Down => Axis::Vertical,
        }
    }

    pub const fn is_forward(self) -> bool {
        matches!(self, Self::Right | Self::Down)
    }

    pub const fn from_delta(self) -> (i32, i32) {
        match self {
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
            Self::Up => (0, -1),
            Self::Down => (0, 1),
        }
    }

    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Up, Dir::Down];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Panes sit side by side, separated by a vertical divider.
    Horizontal,
    /// Panes sit one above the other, separated by a horizontal divider.
    Vertical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_and_intersect() {
        let r = Rect::new(2, 3, 10, 6);
        assert_eq!(r.inner(), Rect::new(3, 4, 8, 4));
        assert_eq!(r.intersect(Rect::new(0, 0, 4, 4)), Rect::new(2, 3, 2, 1));
        assert!(Rect::new(0, 0, 0, 5).is_empty());
        assert_eq!(Rect::new(1, 1, 4, 4).shrink_top(1), Rect::new(1, 2, 4, 3));
    }

    #[test]
    fn directions_map_to_axes() {
        assert_eq!(Dir::Left.axis(), Axis::Horizontal);
        assert_eq!(Dir::Up.axis(), Axis::Vertical);
        assert_eq!(Dir::Left.opposite(), Dir::Right);
    }
}
