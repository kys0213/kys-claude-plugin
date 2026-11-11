//! How the board's columns fit a pane of a given width.

const COLUMN_MIN_WIDTH: u32 = 26;
const COLUMN_GAP: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Row,
    Column,
}

impl Direction {
    pub fn id(self) -> &'static str {
        match self {
            Direction::Row => "row",
            Direction::Column => "column",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub direction: Direction,
    pub column_width: u32,
    pub column_gap: u32,
}

pub fn fit(width: u32, columns: u32) -> Layout {
    let gaps = columns.saturating_sub(1) * COLUMN_GAP;
    if columns > 0 && width >= columns * COLUMN_MIN_WIDTH + gaps {
        Layout {
            direction: Direction::Row,
            column_width: (width - gaps) / columns,
            column_gap: COLUMN_GAP,
        }
    } else {
        Layout {
            direction: Direction::Column,
            column_width: width,
            column_gap: COLUMN_GAP,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_columns_stack_at_full_width() {
        assert_eq!(fit(50, 0).direction, Direction::Column);
        assert_eq!(fit(50, 0).column_width, 50);
    }
}
