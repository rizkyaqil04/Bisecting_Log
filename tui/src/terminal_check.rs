use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Paragraph, Wrap},
    Frame,
};

/// Minimum dimensions for the terminal window.
pub const MIN_WIDTH: u16 = 100;
pub const MIN_HEIGHT: u16 = 35;

/// Check if the terminal is too small.
pub fn is_too_small(area: Rect) -> bool {
    area.width < MIN_WIDTH || area.height < MIN_HEIGHT
}

/// Draw a centered warning message when the terminal is too small.
pub fn draw_too_small_warning(f: &mut Frame, area: Rect) {
    let warning = Paragraph::new(format!(
        "Terminal too small!\n\nCurrent: {}x{}\nMinimum required: {}x{}\n\nPlease resize the window.",
        area.width, area.height, MIN_WIDTH, MIN_HEIGHT
    ))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    .wrap(Wrap { trim: true });

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(7),
            Constraint::Fill(1),
        ])
        .split(area);

    f.render_widget(warning, layout[1]);
}
