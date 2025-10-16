use ratatui::{
    prelude::{Frame, Rect, Layout, Direction, Constraint},
    widgets::{Gauge, Block, Borders, Paragraph},
};

#[derive(Clone)]
pub struct GaugeState {
    pub progress: u8,
    pub status: String,
    pub done: bool,
}

impl GaugeState {
    pub fn new() -> Self {
        Self { progress: 0, status: "Initializing...".into(), done: false }
    }

    pub fn update(&mut self, msg: &str) {
        if msg.starts_with("PROGRESS: ") {
            if let Ok(val) = msg["PROGRESS: ".len()..].parse::<u8>() {
                self.progress = val;
            }
        } else if msg.starts_with("STATUS: ") {
            self.status = msg["STATUS: ".len()..].to_string();
        } else if msg == "DONE" {
            self.done = true;
        }
    }
}

pub fn render_gauge_ui(f: &mut Frame, gauge: &GaugeState) {
    let area = f.area();
    let box_width = (area.width as f32 * 0.6) as u16;
    let box_height = (area.height as f32 * 0.4) as u16;
    let x_offset = (area.width - box_width) / 2;
    let y_offset = (area.height - box_height) / 2;
    let centered_rect = Rect::new(x_offset, y_offset, box_width, box_height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(75),
        ])
        .split(centered_rect);

    let gauge_widget = Gauge::default()
        .block(Block::default().title("Progress").borders(Borders::ALL))
        .ratio(gauge.progress as f64 / 100.0)
        .label(format!("{}%", gauge.progress));

    let status = Paragraph::new(gauge.status.clone())
        .block(Block::default().title("Status").borders(Borders::ALL));

    f.render_widget(gauge_widget, chunks[0]);
    f.render_widget(status, chunks[1]);
}
