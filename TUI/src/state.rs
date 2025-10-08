use ratatui::{
    prelude::*,
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, List, ListItem, Paragraph, Table, Row, Cell},
};
use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};

#[derive(Debug, Clone)]
pub enum Focus {
    Tabs,
    MainList,
    Detail,
    Rename,
}

#[derive(Debug, Clone)]
pub enum OverviewPanel {
    GeneralStats,
    RequestedFiles,
    StaticRequests,
    NotFound,
    VisitorGraph, // panel baru
}

#[derive(Debug, Clone)]
pub struct FileRow {
    pub hits: u32,
    pub visitors: u32,
    pub tx_amount: &'static str,
    pub method: &'static str,
    pub proto: &'static str,
    pub data: &'static str,
}

pub struct AppState {
    pub running: bool,
    pub focus: Focus,
    pub sidebar_index: usize,
    pub sidebar_items: Vec<&'static str>,
    pub tab_items: Vec<Vec<String>>,
    pub content_selection: usize,
    pub rename_buffer: String,
    pub overview_panels: Vec<OverviewPanel>,
    pub requested_files: Vec<FileRow>,
    pub static_requests: Vec<FileRow>,
    pub not_found: Vec<FileRow>,

    // --- palet warna ---
    pub color_header: Color,
    pub color_hits: Color,
    pub color_visitors: Color,
    pub color_method: Color,
    pub color_proto: Color,
    pub color_data: Color,
    pub color_label: Color,
    pub color_value: Color,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            running: true,
            focus: Focus::Tabs,
            sidebar_index: 0,
            sidebar_items: vec!["Overview", "Output", "About"],
            tab_items: vec![
                vec![
                    "General Stats".into(),
                    "Requested Files".into(),
                    "Static Requests".into(),
                    "Not Found URLs".into(),
                    "Visitor Graph".into(),
                ],
                vec!["cluster 0".into(), "cluster 1".into(), "cluster 2".into()],
                vec!["Version Info".into(), "License".into()],
            ],
            content_selection: 0,
            rename_buffer: String::new(),
            overview_panels: vec![
                OverviewPanel::GeneralStats,
                OverviewPanel::RequestedFiles,
                OverviewPanel::StaticRequests,
                OverviewPanel::NotFound,
                OverviewPanel::VisitorGraph,
            ],
            requested_files: vec![
                FileRow { hits: 8, visitors: 2, tx_amount: "0 B", method: "GET", proto: "HTTP/1.1", data: "/index.html" },
                FileRow { hits: 4, visitors: 1, tx_amount: "0 B", method: "GET", proto: "HTTP/1.1", data: "/about.html" },
            ],
            static_requests: vec![
                FileRow { hits: 5, visitors: 1, tx_amount: "0 B", method: "GET", proto: "HTTP/1.1", data: "/static/logo.png" },
            ],
            not_found: vec![
                FileRow { hits: 2, visitors: 1, tx_amount: "0 B", method: "GET", proto: "HTTP/1.1", data: "/missing/page" },
            ],

            color_header: Color::Cyan,
            color_hits: Color::LightGreen,
            color_visitors: Color::Yellow,
            color_method: Color::Magenta,
            color_proto: Color::Blue,
            color_data: Color::White,
            color_label: Color::LightCyan,
            color_value: Color::LightMagenta,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        // text yang mau ditampilkan di footer
        let footer_text = "Keys: q Quit | ←/→ move focus | ↑/↓ navigate | Enter open detail | r rename(Output) | Esc cancel rename";

        // hitung lebar terminal
        let width = area.width as usize;

        // hitung berapa baris yang dibutuhkan
        // + width - 1 supaya hasil ceil (pembulatan ke atas)
        let lines = (footer_text.len() + width - 1) / width;
        let footer_height = lines as u16 + 2; // +2 utk border atas & bawah

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),       // header tetap 3
                Constraint::Min(1),          // body fleksibel
                Constraint::Length(footer_height),
            ])
            .split(area);

        self.draw_header(f, layout[0]);
        self.draw_body(f, layout[1]);
        self.draw_footer(f, layout[2], footer_text);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(25), Constraint::Min(0)])
            .split(area);

        let label_block = Block::bordered().border_set(ratatui::symbols::border::Set {
            top_left: " ",
            top_right: " ",
            bottom_left: " ",
            bottom_right: " ",
            vertical_left: " ",
            vertical_right: " ",
            horizontal_top: "*",
            horizontal_bottom: "*",
        });

        let label = Paragraph::new(Line::from(vec![
            Span::styled("Dashboard ", Style::default().fg(Color::White).bold()),
            Span::styled("by YourName", Style::default().fg(Color::Gray).italic()),
        ]))
        .block(label_block)
        .centered();
        f.render_widget(label, chunks[0]);

        let search_bar = Paragraph::new("[Press / to search]")
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(search_bar, chunks[1]);
    }

    fn draw_body(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(25), Constraint::Min(0)])
            .split(area);

        // Sidebar
        let items: Vec<ListItem> = self.sidebar_items.iter().enumerate().map(|(i, &item)| {
            let mut style = Style::default();
            if i == self.sidebar_index {
                style = style.fg(Color::Yellow);
                if matches!(self.focus, Focus::Tabs) { style = style.reversed(); }
            }
            ListItem::new(Span::styled(item, style))
        }).collect();
        let sidebar = List::new(items).block(Block::default().title("Tabs").borders(Borders::ALL));
        f.render_widget(sidebar, chunks[0]);

        // Main content
        let current_items = &self.tab_items[self.sidebar_index];
        if matches!(self.focus, Focus::Detail) && self.sidebar_index == 0 {
            match self.content_selection {
                0 => self.draw_general_stats(f, chunks[1]),
                1 => self.draw_table_panel(f, chunks[1], "Requested Files", &self.requested_files),
                2 => self.draw_table_panel(f, chunks[1], "Static Requests", &self.static_requests),
                3 => self.draw_table_panel(f, chunks[1], "Not Found URLs", &self.not_found),
                4 => self.draw_graph_table(f, chunks[1]), // panel baru
                _ => {}
            }
        } else if matches!(self.focus, Focus::Detail) {
            let selected = &current_items[self.content_selection];
            let text = Paragraph::new(format!("Detail of '{}'\n[← back]", selected))
                .style(Style::default().fg(Color::White))
                .block(Block::default().borders(Borders::ALL).title("Detail"));
            f.render_widget(text, chunks[1]);
        } else {
            let items: Vec<ListItem> = current_items.iter().enumerate().map(|(i, s)| {
                let mut style = Style::default();
                if i == self.content_selection {
                    style = style.fg(Color::LightCyan);
                    if matches!(self.focus, Focus::MainList) { style = style.reversed(); }
                }
                ListItem::new(Span::styled(s.clone(), style))
            }).collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL)
                .title(self.sidebar_items[self.sidebar_index]));
            f.render_widget(list, chunks[1]);
        }

        if let Focus::Rename = self.focus {
            let popup_v = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(40), Constraint::Length(5), Constraint::Percentage(40)])
                .split(area);
            let popup_h = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Min(40), Constraint::Percentage(20)])
                .split(popup_v[1]);

            let rename_box = Paragraph::new(format!(
                "Rename to: {}\n[Enter] save, [Esc] cancel", self.rename_buffer))
                .style(Style::default().fg(Color::Magenta))
                .block(Block::default().borders(Borders::ALL).title("Rename"));
            f.render_widget(rename_box, popup_h[1]);
        }
    }

    fn draw_general_stats(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(vec![
                Span::styled("Total Requests: ", Style::default().fg(self.color_label)),
                Span::styled("200", Style::default().fg(self.color_value).bold()),
            ]),
            Line::from(vec![
                Span::styled("Unique Visitors: ", Style::default().fg(self.color_label)),
                Span::styled("50", Style::default().fg(self.color_value).bold()),
            ]),
            Line::from(vec![
                Span::styled("Valid Requests: ", Style::default().fg(self.color_label)),
                Span::styled("180", Style::default().fg(self.color_value).bold()),
            ]),
        ];
        let p = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("General Stats"));
        f.render_widget(p, area);
    }

    fn draw_graph_table(&self, f: &mut Frame, area: Rect) {
        let data = vec![
            FileRow { hits: 14, visitors: 1, tx_amount: "0.0 B", method: "GET", proto: "HTTP/1.1", data: "192.168.1.10 ||||||||||||||" },
            FileRow { hits: 10, visitors: 1, tx_amount: "0.0 B", method: "POST", proto: "HTTP/1.1", data: "192.168.1.11 ||||||||" },
            FileRow { hits: 7, visitors: 1, tx_amount: "0.0 B", method: "GET", proto: "HTTP/1.1", data: "192.168.1.12 |||||" },
        ];

        let header = Row::new(vec![
            Cell::from("Hits").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Visitors").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Tx").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Method").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Proto").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Data").style(Style::default().fg(self.color_header).bold()),
        ]);

        let rows: Vec<Row> = data.iter().map(|r| {
            Row::new(vec![
                Cell::from(r.hits.to_string()).style(Style::default().fg(self.color_hits)),
                Cell::from(r.visitors.to_string()).style(Style::default().fg(self.color_visitors)),
                Cell::from(r.tx_amount).style(Style::default().fg(Color::Gray)),
                Cell::from(r.method).style(Style::default().fg(self.color_method)),
                Cell::from(r.proto).style(Style::default().fg(self.color_proto)),
                Cell::from(r.data).style(Style::default().fg(self.color_data)),
            ])
        }).collect();

        let table = Table::new(rows, [
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Min(20),
        ])
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Visitor Hostnames and IPs"));
        f.render_widget(table, area);
    }

    fn draw_table_panel(&self, f: &mut Frame, area: Rect, title: &str, rows: &[FileRow]) {
        let header = Row::new(vec![
            Cell::from("Hits").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Visitors").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Tx").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Method").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Proto").style(Style::default().fg(self.color_header).bold()),
            Cell::from("Data").style(Style::default().fg(self.color_header).bold()),
        ]);

        let rows: Vec<Row> = rows.iter().map(|r| {
            Row::new(vec![
                Cell::from(r.hits.to_string()).style(Style::default().fg(self.color_hits)),
                Cell::from(r.visitors.to_string()).style(Style::default().fg(self.color_visitors)),
                Cell::from(r.tx_amount).style(Style::default().fg(Color::Gray)),
                Cell::from(r.method).style(Style::default().fg(self.color_method)),
                Cell::from(r.proto).style(Style::default().fg(self.color_proto)),
                Cell::from(r.data).style(Style::default().fg(self.color_data)),
            ])
        }).collect();

        let table = Table::new(rows, [
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Min(20),
        ])
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(table, area);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect, text: &str) {
        let footer = Paragraph::new(text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL))
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(footer, area);
    }

    pub fn handle_key(&mut self, key: &KeyEvent) {
        match self.focus {
            Focus::Tabs => match key.code {
                KeyCode::Right => self.focus = Focus::MainList,
                KeyCode::Up if self.sidebar_index > 0 => {
                    self.sidebar_index -= 1; self.content_selection = 0;
                }
                KeyCode::Down if self.sidebar_index + 1 < self.sidebar_items.len() => {
                    self.sidebar_index += 1; self.content_selection = 0;
                }
                KeyCode::Char('q') => self.running = false,
                _ => {}
            },
            Focus::MainList => match key.code {
                KeyCode::Left => self.focus = Focus::Tabs,
                KeyCode::Right | KeyCode::Enter => self.focus = Focus::Detail,
                KeyCode::Up if self.content_selection > 0 => self.content_selection -= 1,
                KeyCode::Down if self.content_selection + 1 < self.tab_items[self.sidebar_index].len() => {
                    self.content_selection += 1;
                }
                KeyCode::Char('r') if self.sidebar_index == 1 => {
                    self.rename_buffer = self.tab_items[1][self.content_selection].clone();
                    self.focus = Focus::Rename;
                }
                KeyCode::Char('q') => self.running = false,
                _ => {}
            },
            Focus::Detail => match key.code {
                KeyCode::Left => self.focus = Focus::MainList,
                KeyCode::Char('q') => self.running = false,
                _ => {}
            },
            Focus::Rename => match key.code {
                KeyCode::Enter => {
                    self.tab_items[1][self.content_selection] = self.rename_buffer.clone();
                    self.focus = Focus::MainList;
                }
                KeyCode::Esc => self.focus = Focus::MainList,
                KeyCode::Backspace => { self.rename_buffer.pop(); }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.rename_buffer.push(c);
                }
                _ => {}
            },
        }
    }
}

