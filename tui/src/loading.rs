use crate::{float::FloatContent, hint::Shortcut, theme::Theme};
use ratatui::{Frame, layout::Rect};

// Simple loading float content that displays a message while background work runs.
pub struct LoadingFloat {
    message: String,
}

impl LoadingFloat {
    pub fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
        }
    }
}

impl FloatContent for LoadingFloat {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        use ratatui::{
            layout::Alignment,
            style::{Modifier, Style},
            widgets::{Block, BorderType, Borders, Clear, Paragraph},
        };

        // Dimmed overlay to prevent background content from showing through
        let overlay = Block::default().style(Style::default().bg(theme.overlay_bg()));
        frame.render_widget(overlay, frame.area());

        // Clear popup area
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Loading ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_color()));

        let p = Paragraph::new(self.message.clone())
            .block(block)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(theme.info_color())
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(p, area);
    }

    fn handle_key_event(&mut self, _key: &ratatui::crossterm::event::KeyEvent) -> bool {
        false
    }

    fn is_finished(&self) -> bool {
        false
    }

    fn get_shortcut_list(&self) -> (&str, Box<[Shortcut]>) {
        ("", Vec::new().into_boxed_slice())
    }
}
