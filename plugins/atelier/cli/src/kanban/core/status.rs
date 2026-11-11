//! A board as `kanban status` lists it: the sessions of the chosen columns in
//! the board's order, as entries or as a table a person reads.

use super::board::{Board, Card, Column};

const SHORT_ID_CHARS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

impl Format {
    pub fn parse(id: &str) -> Result<Format, String> {
        match id {
            "json" => Ok(Format::Json),
            "text" => Ok(Format::Text),
            other => Err(format!(
                "--format 은 json 또는 text 예요 (받은 값: {other})"
            )),
        }
    }
}

/// Columns in the board's order with their cards; with `session`, only that
/// session's card and only the columns that hold it.
pub fn select<'a>(board: &'a Board, session: Option<&str>) -> Vec<(Column, Vec<&'a Card>)> {
    board
        .columns
        .iter()
        .map(|(column, cards)| {
            let shown: Vec<&Card> = cards
                .iter()
                .filter(|card| session.is_none_or(|id| card.session_id == id))
                .collect();
            (*column, shown)
        })
        .filter(|(_, shown)| session.is_none() || !shown.is_empty())
        .collect()
}

fn line(card: &Card) -> String {
    let short: String = card.session_id.chars().take(SHORT_ID_CHARS).collect();
    format!("  {}  {short}", card.headline())
}

pub fn render_text(board: &Board, session: Option<&str>) -> String {
    let mut out = Vec::new();
    for (column, cards) in select(board, session) {
        out.push(column.heading(cards.len()));
        out.extend(cards.into_iter().map(line));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_names_are_json_and_text() {
        assert_eq!(Format::parse("json"), Ok(Format::Json));
        assert_eq!(Format::parse("text"), Ok(Format::Text));
        assert!(Format::parse("yaml").is_err());
    }
}
