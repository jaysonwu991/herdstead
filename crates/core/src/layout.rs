//! Pane trees: splitting, closing, resizing, geometry and focus movement.

use serde::{Deserialize, Serialize};

use crate::geom::{Axis, Dir, Rect};
use crate::ids::PaneId;

/// The direction a split pushes the existing pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDir {
    /// New pane on the right: a vertical divider between them (tmux's `split-window -h`).
    Vertical,
    /// New pane below: a horizontal divider between them.
    Horizontal,
}

impl SplitDir {
    pub const fn axis(self) -> Axis {
        match self {
            Self::Vertical => Axis::Horizontal,
            Self::Horizontal => Axis::Vertical,
        }
    }

    pub const fn from_dir(dir: Dir) -> Self {
        match dir.axis() {
            Axis::Horizontal => Self::Vertical,
            Axis::Vertical => Self::Horizontal,
        }
    }
}

/// A node of the pane tree. Splits own a ratio in `0.05..=0.95`; the divider
/// cell between the two children is not part of either child's rect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Leaf {
        pane: PaneId,
    },
    Split {
        dir: SplitDir,
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// One pane and the rectangle it occupies, in absolute screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanePlacement {
    pub pane: PaneId,
    pub rect: Rect,
}

const MIN_RATIO: f32 = 0.05;
const MAX_RATIO: f32 = 0.95;

fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_nan() {
        return 0.5;
    }
    ratio.clamp(MIN_RATIO, MAX_RATIO)
}

impl Node {
    pub fn leaf(pane: PaneId) -> Self {
        Self::Leaf { pane }
    }

    pub fn split(dir: SplitDir, ratio: f32, first: Node, second: Node) -> Self {
        Self::Split {
            dir,
            ratio: clamp_ratio(ratio),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn first_leaf(&self) -> PaneId {
        match self {
            Self::Leaf { pane } => *pane,
            Self::Split { first, .. } => first.first_leaf(),
        }
    }

    pub fn contains(&self, pane: PaneId) -> bool {
        match self {
            Self::Leaf { pane: p } => *p == pane,
            Self::Split { first, second, .. } => first.contains(pane) || second.contains(pane),
        }
    }

    pub fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Leaf { pane } => out.push(*pane),
            Self::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    /// Replaces `target` with a split holding it and `new_pane`.
    pub fn insert_split(&mut self, target: PaneId, dir: SplitDir, new_pane: PaneId) -> bool {
        match self {
            Self::Leaf { pane } => {
                if *pane != target {
                    return false;
                }
                *self = Self::split(
                    dir,
                    0.5,
                    Self::Leaf { pane: target },
                    Self::Leaf { pane: new_pane },
                );
                true
            }
            Self::Split { first, second, .. } => {
                first.insert_split(target, dir, new_pane)
                    || second.insert_split(target, dir, new_pane)
            }
        }
    }

    /// Removes a leaf, collapsing the split that held it. Returns `false` when
    /// the tree no longer holds any pane.
    pub fn remove(&mut self, target: PaneId) -> bool {
        if !self.contains(target) {
            return true;
        }
        remove_in(self, target)
    }

    /// Swaps two panes' positions in the tree. Both must be present.
    pub fn swap(&mut self, a: PaneId, b: PaneId) -> bool {
        if a == b || !self.contains(a) || !self.contains(b) {
            return false;
        }
        swap_in(self, a, b)
    }

    /// Adjusts the nearest ancestor split whose axis matches `dir`, growing the
    /// side the pane sits on. `cells` is the requested change in cells; the
    /// fraction is derived from the split's current extent in `area`.
    pub fn resize(&mut self, target: PaneId, dir: Dir, cells: f32, area: Rect) -> bool {
        match self {
            Self::Leaf { .. } => false,
            Self::Split {
                dir: split_dir,
                ratio,
                first,
                second,
            } => {
                let (first_area, second_area) = split_areas(area, *split_dir, *ratio);
                // The divider must move away from the pane being grown, so the
                // pane has to sit on the side the split grows from.
                let matches_axis = split_dir.axis() == dir.axis();
                let on_toward_side = if dir.is_forward() {
                    first.contains(target)
                } else {
                    second.contains(target)
                };
                if matches_axis && on_toward_side {
                    let extent = match dir.axis() {
                        Axis::Horizontal => area.width,
                        Axis::Vertical => area.height,
                    };
                    let extent = f32::from(extent).max(1.0);
                    let signed = if dir.is_forward() { cells } else { -cells };
                    *ratio = clamp_ratio(*ratio + signed / extent);
                    true
                } else if first.contains(target) {
                    first.resize(target, dir, cells, first_area)
                } else if second.contains(target) {
                    second.resize(target, dir, cells, second_area)
                } else {
                    false
                }
            }
        }
    }

    /// Lays the tree out over `area`, leaving one cell between children.
    pub fn placements(&self, area: Rect) -> Vec<PanePlacement> {
        let mut out = Vec::new();
        self.place(area, &mut out);
        out
    }

    fn place(&self, area: Rect, out: &mut Vec<PanePlacement>) {
        match self {
            Self::Leaf { pane } => out.push(PanePlacement {
                pane: *pane,
                rect: area,
            }),
            Self::Split {
                dir,
                ratio,
                first,
                second,
            } => {
                let (a, b) = split_areas(area, *dir, *ratio);
                first.place(a, out);
                second.place(b, out);
            }
        }
    }
}

fn remove_in(node: &mut Node, target: PaneId) -> bool {
    match node {
        Node::Leaf { pane } => *pane != target,
        Node::Split { .. } => {
            let collapse = {
                let Node::Split { first, second, .. } = node else {
                    return true;
                };
                if first.contains(target) {
                    (!remove_in(first, target)).then(|| (**second).clone())
                } else if second.contains(target) {
                    (!remove_in(second, target)).then(|| (**first).clone())
                } else {
                    None
                }
            };
            match collapse {
                Some(sibling) => {
                    *node = sibling;
                    true
                }
                None => true,
            }
        }
    }
}

fn swap_in(node: &mut Node, a: PaneId, b: PaneId) -> bool {
    match node {
        Node::Leaf { pane } => {
            if *pane == a {
                *pane = b;
                true
            } else if *pane == b {
                *pane = a;
                true
            } else {
                false
            }
        }
        Node::Split { first, second, .. } => {
            // Both subtrees are visited: the two panes are in different leaves.
            let swapped_first = swap_in(first, a, b);
            let swapped_second = swap_in(second, a, b);
            swapped_first || swapped_second
        }
    }
}

/// Rectangles for both children of a split, removing the divider cell.
pub fn split_areas(area: Rect, dir: SplitDir, ratio: f32) -> (Rect, Rect) {
    let ratio = clamp_ratio(ratio);
    match dir {
        SplitDir::Vertical => {
            if area.width < 3 {
                let left = area.width / 2;
                return (
                    Rect::new(area.x, area.y, left, area.height),
                    Rect::new(area.x + left, area.y, area.width - left, area.height),
                );
            }
            let usable = area.width - 1;
            let first = ((f32::from(usable) * ratio).round() as u16).clamp(1, usable - 1);
            (
                Rect::new(area.x, area.y, first, area.height),
                Rect::new(area.x + first + 1, area.y, usable - first, area.height),
            )
        }
        SplitDir::Horizontal => {
            if area.height < 3 {
                let top = area.height / 2;
                return (
                    Rect::new(area.x, area.y, area.width, top),
                    Rect::new(area.x, area.y + top, area.width, area.height - top),
                );
            }
            let usable = area.height - 1;
            let first = ((f32::from(usable) * ratio).round() as u16).clamp(1, usable - 1);
            (
                Rect::new(area.x, area.y, area.width, first),
                Rect::new(area.x, area.y + first + 1, area.width, usable - first),
            )
        }
    }
}

/// The pane whose centre is nearest to `from` in direction `dir`, requiring
/// overlap on the perpendicular axis so focus follows what the eye sees.
pub fn neighbor(placements: &[PanePlacement], from: PaneId, dir: Dir) -> Option<PaneId> {
    let current = placements.iter().find(|p| p.pane == from)?.rect;
    let (cx, cy) = current.center();
    let mut best: Option<(i64, i64, PaneId)> = None;
    for candidate in placements {
        if candidate.pane == from {
            continue;
        }
        let rect = candidate.rect;
        let (px, py) = rect.center();
        let (dx, dy) = (px - cx, py - cy);
        let in_direction = match dir {
            Dir::Left => dx < 0,
            Dir::Right => dx > 0,
            Dir::Up => dy < 0,
            Dir::Down => dy > 0,
        };
        if !in_direction {
            continue;
        }
        let overlaps = match dir.axis() {
            Axis::Horizontal => {
                let top = current.y.max(rect.y);
                let bottom = current.bottom().min(rect.bottom());
                bottom > top
            }
            Axis::Vertical => {
                let left = current.x.max(rect.x);
                let right = current.right().min(rect.right());
                right > left
            }
        };
        let distance = match dir.axis() {
            Axis::Horizontal => i64::from(dx.abs()),
            Axis::Vertical => i64::from(dy.abs()),
        };
        // Prefer panes that line up with the current one, then the closest.
        let score = (if overlaps { 0 } else { 1 }, distance);
        let replace = match best {
            None => true,
            Some((best_score, best_distance, _)) => {
                score.0 < best_score || (score.0 == best_score && score.1 < best_distance)
            }
        };
        if replace {
            best = Some((score.0, score.1, candidate.pane));
        }
    }
    best.map(|(_, _, pane)| pane)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(node: &Node) -> Vec<PaneId> {
        node.leaves()
    }

    #[test]
    fn splitting_places_two_panes_side_by_side() {
        let mut tree = Node::leaf(PaneId(1));
        assert!(tree.insert_split(PaneId(1), SplitDir::Vertical, PaneId(2)));
        let placements = tree.placements(Rect::new(0, 0, 81, 20));
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[0].rect, Rect::new(0, 0, 40, 20));
        assert_eq!(placements[1].rect, Rect::new(41, 0, 40, 20));
        assert_eq!(leaves(&tree), vec![PaneId(1), PaneId(2)]);
    }

    #[test]
    fn splitting_horizontal_stacks_rows() {
        let mut tree = Node::leaf(PaneId(1));
        assert!(tree.insert_split(PaneId(1), SplitDir::Horizontal, PaneId(2)));
        let placements = tree.placements(Rect::new(0, 0, 20, 11));
        assert_eq!(placements[0].rect, Rect::new(0, 0, 20, 5));
        assert_eq!(placements[1].rect, Rect::new(0, 6, 20, 5));
    }

    #[test]
    fn closing_collapses_and_promotes_sibling() {
        let mut tree = Node::leaf(PaneId(1));
        tree.insert_split(PaneId(1), SplitDir::Vertical, PaneId(2));
        tree.insert_split(PaneId(2), SplitDir::Horizontal, PaneId(3));
        assert!(tree.remove(PaneId(2)));
        assert_eq!(leaves(&tree), vec![PaneId(1), PaneId(3)]);
        assert!(tree.remove(PaneId(1)));
        assert_eq!(leaves(&tree), vec![PaneId(3)]);
        // Removing the last pane reports an empty tree and leaves the node in
        // place; the caller then drops the tab.
        assert!(!tree.remove(PaneId(3)));
    }

    #[test]
    fn focus_follows_geometry() {
        let mut tree = Node::leaf(PaneId(1));
        tree.insert_split(PaneId(1), SplitDir::Vertical, PaneId(2));
        tree.insert_split(PaneId(2), SplitDir::Horizontal, PaneId(3));
        let placements = tree.placements(Rect::new(0, 0, 80, 24));
        assert_eq!(
            neighbor(&placements, PaneId(1), Dir::Right),
            Some(PaneId(2))
        );
        assert_eq!(neighbor(&placements, PaneId(2), Dir::Down), Some(PaneId(3)));
        assert_eq!(neighbor(&placements, PaneId(3), Dir::Left), Some(PaneId(1)));
        assert_eq!(neighbor(&placements, PaneId(1), Dir::Left), None);
    }

    #[test]
    fn resize_grows_the_side_the_pane_is_on() {
        let mut tree = Node::leaf(PaneId(1));
        tree.insert_split(PaneId(1), SplitDir::Vertical, PaneId(2));
        let area = Rect::new(0, 0, 100, 20);
        let before = tree.placements(area)[0].rect.width;
        assert!(tree.resize(PaneId(1), Dir::Right, 10.0, area));
        let wider = tree.placements(area)[0].rect.width;
        assert!(wider > before, "{wider} should be wider than {before}");
        assert!((58..=60).contains(&wider), "{wider}");
        assert!(tree.resize(PaneId(1), Dir::Right, -10.0, area));
        let narrower = tree.placements(area)[0].rect.width;
        assert!((48..=51).contains(&narrower), "{narrower}");
        // A pane with no divider on that side cannot be resized that way.
        assert!(!tree.resize(PaneId(1), Dir::Left, 10.0, area));
    }

    #[test]
    fn swap_exchanges_panes() {
        let mut tree = Node::leaf(PaneId(1));
        tree.insert_split(PaneId(1), SplitDir::Vertical, PaneId(2));
        assert!(tree.swap(PaneId(1), PaneId(2)));
        assert_eq!(leaves(&tree), vec![PaneId(2), PaneId(1)]);
    }

    #[test]
    fn tiny_areas_do_not_panic() {
        let mut tree = Node::leaf(PaneId(1));
        tree.insert_split(PaneId(1), SplitDir::Vertical, PaneId(2));
        let placements = tree.placements(Rect::new(0, 0, 1, 1));
        assert_eq!(placements.len(), 2);
        let placements = tree.placements(Rect::new(0, 0, 2, 2));
        assert_eq!(placements.len(), 2);
    }
}
