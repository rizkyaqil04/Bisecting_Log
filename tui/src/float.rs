use crate::{hint::Shortcut, theme::Theme};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    Frame,
};

pub trait FloatContent {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme);
    fn handle_key_event(&mut self, key: &KeyEvent) -> bool;
    fn is_finished(&self) -> bool;
    fn get_shortcut_list(&self) -> (&str, Box<[Shortcut]>);
}

#[derive(Debug, Clone, Copy)]
pub enum FloatMode {
    /// Default behavior — based on screen percentage
    Percent(u16, u16),
    /// Fixed size (in terminal character units)
    Absolute(u16, u16),
}

pub struct Float<Content: FloatContent + ?Sized> {
    pub content: Box<Content>,
    mode: FloatMode,
}

impl<Content: FloatContent + ?Sized> Float<Content> {
    /// Create a floating window using percentage-based size
    pub fn new(content: Box<Content>, width_percent: u16, height_percent: u16) -> Self {
        Self {
            content,
            mode: FloatMode::Percent(width_percent, height_percent),
        }
    }

    /// Create a floating window using absolute width/height
    pub fn new_absolute(content: Box<Content>, width: u16, height: u16) -> Self {
        Self {
            content,
            mode: FloatMode::Absolute(width, height),
        }
    }

    /// Calculate centered popup area based on mode
    fn floating_window(&self, area: Rect) -> Rect {
        match self.mode {
            FloatMode::Percent(wp, hp) => {
                let hor_float = Layout::horizontal([
                    Constraint::Percentage((100 - wp) / 2),
                    Constraint::Percentage(wp),
                    Constraint::Percentage((100 - wp) / 2),
                ])
                .split(area)[1];

                Layout::vertical([
                    Constraint::Percentage((100 - hp) / 2),
                    Constraint::Percentage(hp),
                    Constraint::Percentage((100 - hp) / 2),
                ])
                .split(hor_float)[1]
            }
            FloatMode::Absolute(w, h) => {
                let w = w.min(area.width);
                let h = h.min(area.height);
                let x = area.x + (area.width.saturating_sub(w)) / 2;
                let y = area.y + (area.height.saturating_sub(h)) / 2;
                Rect::new(x, y, w, h)
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, parent_area: Rect, theme: &Theme) {
        let popup_area = self.floating_window(parent_area);
        self.content.draw(frame, popup_area, theme);
    }

    // Returns true if the floating window is finished
    pub fn handle_key_event(&mut self, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter
            | KeyCode::Char('p')
            | KeyCode::Char('d')
            | KeyCode::Char('g')
            | KeyCode::Char('q')
            | KeyCode::Esc
                if self.content.is_finished() =>
            {
                true
            }
            _ => self.content.handle_key_event(key),
        }
    }

    pub fn get_shortcut_list(&self) -> (&str, Box<[Shortcut]>) {
        self.content.get_shortcut_list()
    }
}
