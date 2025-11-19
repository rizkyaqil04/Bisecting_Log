use crate::{float::FloatContent, hint::Shortcut, theme::Theme};
use ratatui::{Frame, layout::Rect};

/// Simple floating window for confirming application exit.
/// Press [y] to confirm exit, [n] or [Esc] to cancel.
pub struct ConfirmQuit {
    finished: bool,
    confirmed: bool,
}

impl ConfirmQuit {
    pub fn new() -> Self {
        Self {
            finished: false,
            confirmed: false,
        }
    }

    pub fn confirmed(&self) -> bool {
        self.confirmed
    }
}

impl FloatContent for ConfirmQuit {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        use ratatui::{
            layout::Alignment,
            style::{Modifier, Style},
            widgets::{Block, BorderType, Borders, Clear, Paragraph},
        };

        // Dimmed overlay to prevent background content from showing through
        let overlay = Block::default().style(Style::default().bg(theme.overlay_bg()));
        frame.render_widget(overlay, frame.area());

        // Clear popup area (erase buffer content)
        frame.render_widget(Clear, area);

        // Draw the popup window
        let block = Block::default()
            .title(" Exit Confirmation ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_color()));

        let text =
            Paragraph::new("Are you sure you want to exit?\n\n\n[y] Yes              [n] No")
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(theme.info_color())
                        .add_modifier(Modifier::BOLD),
                )
                .block(block);

        frame.render_widget(text, area);
    }

    fn handle_key_event(&mut self, key: &ratatui::crossterm::event::KeyEvent) -> bool {
        use ratatui::crossterm::event::KeyCode::*;
        match key.code {
            Char('y') => {
                self.confirmed = true;
                self.finished = true;
                true
            }
            Char('n') | Esc => {
                self.finished = true;
                false
            }
            _ => false,
        }
    }

    fn is_finished(&self) -> bool {
        self.finished
    }

    fn get_shortcut_list(&self) -> (&str, Box<[Shortcut]>) {
        (
            "Quit Confirmation",
            crate::shortcuts!(("Confirm quit", ["y"]), ("Cancel", ["n", "Esc"])),
        )
    }
}
