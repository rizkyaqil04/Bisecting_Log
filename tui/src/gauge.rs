use ratatui::{
    prelude::{Constraint, Direction, Frame, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Gauge, Paragraph},
};

#[derive(Clone)]
pub struct GaugeState {
    pub progress: u8,
    pub status: String,
    pub done: bool,
    // optional transient message coming from the python process
    pub message: Option<String>,
    // whether the message is an error (for coloring)
    pub message_error: bool,
}

impl GaugeState {
    pub fn new() -> Self {
        Self {
            progress: 0,
            status: "Initializing...".into(),
            done: false,
            message: None,
            message_error: false,
        }
    }

    /// Update the gauge state from a single message line coming from the python runner.
    /// Only recognised / well-formed messages will be applied; other lines are ignored.
    pub fn update(&mut self, msg: &str) {
        // Expected canonical messages are produced by `ProgressManager` in the python app:
        //  - "PROGRESS: <pct>"
        //  - "STATUS: <text>"
        //  - "DONE"
        // We also accept explicit error lines prefixed with "ERROR: " or lines containing
        // tracebacks/exceptions and display them in the message box (colored).

        if let Some(rest) = msg.strip_prefix("PROGRESS: ") {
            if let Ok(val) = rest.parse::<u8>() {
                self.progress = val;
            }
            return;
        }

        if let Some(rest) = msg.strip_prefix("STATUS: ") {
            self.status = rest.to_string();
            return;
        }

        if msg == "DONE" {
            self.done = true;
            return;
        }

        if let Some(rest) = msg.strip_prefix("ERROR: ") {
            self.message = Some(rest.to_string());
            self.message_error = true;
            return;
        }

        // Heuristic: treat any line that looks like a Python traceback or exception as an error
        if msg.contains("Traceback")
            || msg.contains("Exception")
            || msg.contains("Traceback (most recent call last)")
        {
            self.message = Some(msg.to_string());
            self.message_error = true;
            return;
        }

        // Unrecognised lines are ignored so they cannot overwrite the UI.
        // This prevents arbitrary output from the python process from corrupting the
        // gauge rendering.
    }
}

pub fn render_gauge_ui(f: &mut Frame, gauge: &GaugeState) {
    // Place the gauge UI in a centered box with margins so it doesn't occupy
    // the entire terminal. This keeps a stable, centered look.
    let area = f.area();

    // compute box size (percentage of terminal with sensible minima)
    let box_width = {
        let w = (area.width as f32 * 0.6) as u16;
        if w < 40 {
            std::cmp::max(20, area.width.saturating_sub(8))
        } else {
            std::cmp::min(w, area.width.saturating_sub(4))
        }
    };
    let box_height = {
        let h = (area.height as f32 * 0.45) as u16;
        if h < 8 {
            std::cmp::max(6, area.height.saturating_sub(6))
        } else {
            std::cmp::min(h, area.height.saturating_sub(2))
        }
    };

    let x_offset = (area.width.saturating_sub(box_width)) / 2;
    let y_offset = (area.height.saturating_sub(box_height)) / 2;
    let centered = Rect::new(x_offset, y_offset, box_width, box_height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            // fixed gauge height
            Constraint::Length(3),
            // fixed status area
            Constraint::Length(3),
            // remaining area for messages / details
            Constraint::Min(3),
        ])
        .split(centered);

    // Progress gauge (fixed height)
    let gauge_widget = Gauge::default()
        .block(Block::default().title("Progress").borders(Borders::ALL))
        .ratio((gauge.progress.min(100) as f64) / 100.0)
        .label(format!("{}%", gauge.progress));

    // Status line (single-line fixed)
    let status = Paragraph::new(gauge.status.clone())
        .block(Block::default().title("Status").borders(Borders::ALL));

    f.render_widget(gauge_widget, chunks[0]);
    f.render_widget(status, chunks[1]);

    // Message box (optional). Colorize if it's an error.
    let msg_block = Block::default()
        .title("Log Message")
        .title_style(Style::default().fg(crate::theme::Theme::Default.table_text()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(crate::theme::Theme::Default.table_text())); // border tetap theme table text

    if let Some(msg) = &gauge.message {
        let style = if gauge.message_error {
            Style::default().fg(crate::theme::Theme::Default.danger_color())
        } else {
            Style::default().fg(crate::theme::Theme::Default.info_color())
        };

        let paragraph = Paragraph::new(msg.clone())
            .style(style) // hanya isi yang diwarnai
            .block(msg_block); // border tidak ikut berubah

        f.render_widget(paragraph, chunks[2]);
    } else {
        let empty = Paragraph::new("").block(msg_block);
        f.render_widget(empty, chunks[2]);
    }
}
