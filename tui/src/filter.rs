use ratatui::{
    prelude::*,
    widgets::{Block, Paragraph},
    symbols::border,
};
use unicode_width::UnicodeWidthChar;

/// Actions triggered by search bar input
pub enum SearchAction {
    None,
    Exit,
    Update,
}

/// Comparison / matching operations supported
#[derive(Debug, Clone, PartialEq)]
pub enum SearchOp {
    Eq,       // contains (partial match)
    EqExact,  // exact equality (==)
    NotEq,    // not equal (!=)
    Gt,
    Lt,
    Ge,
    Le,
    Contains, // fallback contains-anywhere
}

/// A single parsed condition, e.g. `size>200`
#[derive(Debug, Clone)]
pub struct SearchExpr {
    pub key: String,
    pub op: SearchOp,
    pub value: String,
}

/// Multiple conditions combined, e.g. `method=GET size>200`
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub exprs: Vec<SearchExpr>,
}

#[derive(Default)]
pub struct Filter {
    in_search: bool,
    input: Vec<char>,
    cursor: usize,
}

impl Filter {
    pub fn activate(&mut self) {
        self.in_search = true;
    }
    pub fn deactivate(&mut self) {
        self.in_search = false;
    }
    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }
    pub fn term(&self) -> String {
        self.input.iter().collect()
    }
    pub fn active(&self) -> bool {
        self.in_search
    }

    /// Parse query into multiple expressions (e.g. `method=GET size>200`)
    pub fn parsed_query(&self) -> Option<SearchQuery> {
        let term_string = self.term();
        let text = term_string.trim();

        if text.is_empty() {
            return None;
        }

        let mut exprs = Vec::new();
        let parts = text.split_whitespace();

        for part in parts {
            // Prioritize longer operators first
            let ops = ["==", "!=", ">=", "<=", ">", "<", "="];
            for &op in &ops {
                if let Some((key, val)) = part.split_once(op) {
                    let key = key.trim().to_lowercase();
                    let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
                    let op_enum = match op {
                        "==" => SearchOp::EqExact,
                        "!=" => SearchOp::NotEq,
                        "=" => SearchOp::Eq,
                        ">" => SearchOp::Gt,
                        "<" => SearchOp::Lt,
                        ">=" => SearchOp::Ge,
                        "<=" => SearchOp::Le,
                        _ => SearchOp::Contains,
                    };
                    exprs.push(SearchExpr { key, op: op_enum, value: val });
                    break;
                }
            }
        }

        // Fallback — if user typed just one word (no operator)
        if exprs.is_empty() && !text.is_empty() {
            exprs.push(SearchExpr {
                key: String::new(),
                op: SearchOp::Contains,
                value: text.to_string(),
            });
        }

        if exprs.is_empty() {
            None
        } else {
            Some(SearchQuery { exprs })
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.in_search || !self.input.is_empty() {
            self.term()
        } else {
            "Press / to search".into()
        };
        let p = Paragraph::new(hint)
            .block(
                Block::bordered()
                    .title(" Search ")
                    .border_set(border::ROUNDED)
                    .border_type(ratatui::widgets::BorderType::Rounded),
            );
        frame.render_widget(p, area);

        if self.in_search {
            let w: u16 = self
                .input
                .iter()
                .take(self.cursor)
                .map(|c| c.width().unwrap_or(1) as u16)
                .sum();
            frame.set_cursor_position(Position::new(area.x + 1 + w, area.y + 1));
        }
    }

    pub fn handle_key(&mut self, key: &ratatui::crossterm::event::KeyEvent) -> SearchAction {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};
        match key.code {
            KeyCode::Esc | KeyCode::Enter => return SearchAction::Exit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.clear();
                return SearchAction::Exit;
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.input.remove(self.cursor);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Char(ch) => {
                self.input.insert(self.cursor, ch);
                self.cursor += 1;
            }
            _ => return SearchAction::None,
        }
        SearchAction::Update
    }
}
