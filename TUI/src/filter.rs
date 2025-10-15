use ratatui::{
    prelude::*,
    crossterm::event::{KeyEvent, KeyCode, KeyModifiers},
    symbols::border,
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthChar;
use crate::theme::Theme;

pub enum SearchAction {
    None,
    Exit,
    Update(Vec<String>), // hasil filter dikembalikan
}

pub struct Filter {
    pub search_input: Vec<char>,
    pub in_search_mode: bool,
    pub input_position: usize,
}

impl Filter {
    pub fn new() -> Self {
        Self {
            search_input: vec![],
            in_search_mode: false,
            input_position: 0,
        }
    }

    pub fn activate(&mut self) {
        self.in_search_mode = true;
        if self.input_position > self.search_input.len() {
            self.input_position = self.search_input.len();
        }
    }

    pub fn deactivate(&mut self) {
        self.in_search_mode = false;
        self.search_input.clear();
        self.input_position = 0;
    }

    /// core logic filter
    fn apply_filter(&self, data: &[String]) -> Vec<String> {
        if self.search_input.is_empty() {
            return data.to_vec(); // restore otomatis
        }
        let query = self.search_input.iter().collect::<String>().to_lowercase();
        data.iter()
            .filter(|item| item.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    pub fn handle_key(
        &mut self,
        key: &KeyEvent,
        data: &[String],
    ) -> SearchAction {
        // toggle search
        if !self.in_search_mode {
            if let KeyCode::Char('/') = key.code {
                self.activate();
                return SearchAction::Update(self.apply_filter(data));
            }
            return SearchAction::None;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.deactivate();
                return SearchAction::Exit;
            }
            KeyCode::Char(c) => {
                self.search_input.insert(self.input_position, c);
                self.input_position += 1;
                let filtered = self.apply_filter(data);
                SearchAction::Update(filtered)
            }
            KeyCode::Backspace => {
                if self.input_position > 0 {
                    self.input_position -= 1;
                    self.search_input.remove(self.input_position);
                }
                let filtered = self.apply_filter(data);
                SearchAction::Update(filtered)
            }
            KeyCode::Enter => {
                self.in_search_mode = false;
                SearchAction::Exit
            }
            KeyCode::Esc => {
                self.deactivate();
                SearchAction::Update(data.to_vec())
            }
            KeyCode::Left => {
                self.input_position = self.input_position.saturating_sub(1);
                SearchAction::None
            }
            KeyCode::Right => {
                if self.input_position < self.search_input.len() {
                    self.input_position += 1;
                }
                SearchAction::None
            }
            _ => SearchAction::None,
        }
    }

    pub fn draw_searchbar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let display_text = if !self.in_search_mode && self.search_input.is_empty() {
            Span::raw("Press / to search")
        } else {
            let input = self.search_input.iter().collect::<String>();
            Span::styled(input, Style::default().fg(theme.focused_color()))
        };

        let color = if self.in_search_mode {
            theme.focused_color()
        } else {
            theme.unfocused_color()
        };

        let search_bar = Paragraph::new(display_text)
            .block(
                Block::bordered()
                    .border_set(border::ROUNDED)
                    .title(" Search "),
            )
            .style(Style::default().fg(color));

        f.render_widget(search_bar, area);

        if self.in_search_mode {
            let cursor_offset: u16 = self.search_input
                .iter()
                .map(|c| c.width().unwrap_or(1) as u16)
                .sum();
            f.set_cursor_position(Position::new(area.x + 1 + cursor_offset, area.y + 1));
        }
    }
}
