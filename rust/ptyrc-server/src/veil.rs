// SPDX-License-Identifier: GPL-3.0-or-later
//! The key that hides all (`plan/store-model.md`, principle 7): what
//! the apps drew hidden, then told apart, then shown again. Pure: a
//! grid in, the same grid out.

use ptyrc_vt::{Attrs, Cell, Color, Grid, Rect, Style, char_width, str_width};
use std::collections::BTreeMap;

/// What is shown of what the apps drew.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Veil {
    /// As drawn.
    #[default]
    Off,
    /// Nothing: the program's text alone.
    Hidden,
    /// Each owner's cells tinted, its name on them: owner id to name.
    Told(BTreeMap<u64, String>),
}

impl Veil {
    /// The next press of the key.
    pub fn next(&self, names: BTreeMap<u64, String>) -> Veil {
        match self {
            Veil::Off => Veil::Hidden,
            Veil::Hidden => Veil::Told(names),
            Veil::Told(_) => Veil::Off,
        }
    }
}

/// Tints of the owners, in turn; never blue, the colour of the mark.
const TINTS: [u8; 5] = [52, 22, 53, 23, 58];

/// The mark of the driver: bold white on blue, as its pop-up.
pub fn mark() -> Style {
    Style {
        fg: Color::Indexed(15),
        bg: Color::Indexed(4),
        attrs: Attrs(Attrs::BOLD),
        ..Style::default()
    }
}

/// Over `grid`: each box of an owner tinted; its name, as the driver's
/// mark, at its first box. Nothing an app drew can cover a name: they
/// are laid last, and a tinted cell is never blue.
pub fn tell(grid: &mut Grid, boxes: &[(Rect, u64)], names: &BTreeMap<u64, String>) {
    let screen = grid.rect();
    let mut owners: Vec<u64> = boxes.iter().map(|(_, o)| *o).collect();
    owners.sort_unstable();
    owners.dedup();
    let tint = |owner: u64| {
        let i = owners.iter().position(|o| *o == owner).unwrap_or(0);
        Color::Indexed(TINTS[i % TINTS.len()])
    };
    for (rect, owner) in boxes {
        let rect = rect.intersect(&screen);
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                grid.cell_mut(x, y).style.bg = tint(*owner);
            }
        }
    }
    for owner in &owners {
        let first = (boxes.iter().filter(|(_, o)| o == owner))
            .map(|(r, _)| r.intersect(&screen))
            .filter(|r| !r.is_empty())
            .min_by_key(|r| (r.y, r.x));
        let Some(rect) = first else {
            continue;
        };
        let name = names.get(owner).map_or("?", String::as_str);
        let label = format!(" {name} ");
        let width = u16::try_from(str_width(&label)).unwrap_or(u16::MAX);
        let mut x = rect.x.min(screen.right().saturating_sub(width));
        for c in label.chars() {
            let w = char_width(c) as u16;
            if w == 0 || c.is_control() {
                continue;
            }
            if x + w > screen.right() {
                break;
            }
            grid.put(x, rect.y, Cell::glyph(c, w as u8, mark()));
            x += w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(grid: &Grid, y: u16) -> String {
        (0..grid.cols())
            .map(|x| grid.cell(x, y).text.to_string())
            .collect()
    }

    #[test]
    fn three_presses() {
        let names = BTreeMap::from([(1, "a".to_string())]);
        let hidden = Veil::Off.next(names.clone());
        assert_eq!(hidden, Veil::Hidden);
        let told = hidden.next(names.clone());
        assert_eq!(told, Veil::Told(names.clone()));
        assert_eq!(told.next(names), Veil::Off);
    }

    #[test]
    fn each_owner_tinted_and_named_once() {
        let mut grid = Grid::new(20, 3);
        let names = BTreeMap::from([(7, "native/cat".to_string())]);
        let boxes = [
            (Rect::new(2, 1, 3, 1), 7),
            (Rect::new(0, 2, 2, 1), 7),
            (Rect::new(15, 0, 9, 1), 9),
        ];
        tell(&mut grid, &boxes, &names);
        assert_eq!(row(&grid, 1).trim_end(), "   native/cat");
        assert_eq!(grid.cell(2, 1).style, mark());
        assert_ne!(grid.cell(0, 2).style.bg, Color::Default);
        assert_ne!(grid.cell(0, 2).style.bg, mark().bg);
        // Not named: `?`; a name at the edge is moved in.
        assert_eq!(&row(&grid, 0)[15..], " ?   ");
        assert_ne!(grid.cell(0, 2).style.bg, grid.cell(19, 0).style.bg);
    }
}
