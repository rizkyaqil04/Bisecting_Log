use crate::{float::FloatContent, hint::Shortcut, theme::Theme};
use ratatui::{Frame, layout::Rect};

pub struct DataDetail {
    pub lines: Vec<String>,
    finished: bool,
}

impl DataDetail {
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            lines,
            finished: false,
        }
    }
}

impl FloatContent for DataDetail {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        use ratatui::{
            layout::Alignment,
            style::Style,
            widgets::{Block, Borders, Clear, Paragraph},
        };

        // Dim overlay
        let overlay = Block::default().style(Style::default().bg(theme.overlay_bg()));
        frame.render_widget(overlay, frame.area());
        frame.render_widget(Clear, area);

        let text = Paragraph::new(self.lines.join("\n"))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Detail ")
                    .border_type(ratatui::widgets::BorderType::Rounded),
            )
            .style(Style::default().fg(theme.info_color()))
            .alignment(Alignment::Left);

        frame.render_widget(text, area);
    }

    fn handle_key_event(&mut self, key: &ratatui::crossterm::event::KeyEvent) -> bool {
        use ratatui::crossterm::event::KeyCode::*;
        match key.code {
            Char('q') | Esc => {
                self.finished = true;
                true
            }
            _ => false,
        }
    }

    fn is_finished(&self) -> bool {
        self.finished
    }

    fn get_shortcut_list(&self) -> (&str, Box<[Shortcut]>) {
        ("Detail", crate::shortcuts!(("Close", ["q", "Esc"]),))
    }
}
