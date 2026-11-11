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

fn picked(card: &Card, session: Option<&str>) -> bool {
    session.is_none_or(|id| card.session_id == id)
}

/// Cards in column order, then card order; only `session` when it is given.
pub fn select<'a>(board: &'a Board, session: Option<&str>) -> Vec<(Column, &'a Card)> {
    board
        .columns
        .iter()
        .flat_map(|(column, cards)| cards.iter().map(move |card| (*column, card)))
        .filter(|(_, card)| picked(card, session))
        .collect()
}

fn line(card: &Card) -> String {
    let short: String = card.session_id.chars().take(SHORT_ID_CHARS).collect();
    let mut parts = vec![card.label.clone(), short];
    parts.extend(card.reason.clone());
    parts.push(card.age.clone());
    format!("  {}", parts.join("  "))
}

pub fn render_text(board: &Board, session: Option<&str>) -> String {
    let mut out = Vec::new();
    for (column, cards) in &board.columns {
        let shown: Vec<&Card> = cards.iter().filter(|c| picked(c, session)).collect();
        if session.is_some() && shown.is_empty() {
            continue;
        }
        out.push(format!("{} {}", column.title(), shown.len()));
        out.extend(shown.into_iter().map(line));
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
