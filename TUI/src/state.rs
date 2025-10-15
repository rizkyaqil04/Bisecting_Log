use ratatui::{
    prelude::*,
    style::{Style, Stylize},
    widgets::{Block, Borders, List, ListItem, Paragraph, Table, Row, Cell},
};
use ratatui::crossterm::event::{KeyEvent, KeyCode, KeyModifiers};
use crate::theme::Theme;
use crate::filter::{Filter, SearchAction};
use crate::io::{read_latest_csv, group_by_cluster, ClusteredLog};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
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
    VisitorGraph,
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
    pub tab_items_original: Vec<Vec<String>>, // <--- tambahkan ini
    pub content_selection: usize,
    pub rename_buffer: String,
    pub overview_panels: Vec<OverviewPanel>,
    pub requested_files: Vec<FileRow>,
    pub static_requests: Vec<FileRow>,
    pub not_found: Vec<FileRow>,
    pub theme: Theme,
    pub filter: Filter,

    // Hasil klasterisasi
    pub clusters: HashMap<u8, Vec<ClusteredLog>>,

    // Mode search di dalam DETAIL cluster (Output → Detail)
    // Ketika Some(cid, rows), rows adalah hasil filter baris cluster cid
    cluster_backup: Option<(u8, Vec<ClusteredLog>)>,
    filtered_cluster_rows: Option<(u8, Vec<ClusteredLog>)>,

    pub scroll_offset: usize, // <--- tambahkan ini
}

impl AppState {
    pub fn new() -> Self {
        let logs = read_latest_csv("../outputs").unwrap_or_default();
        let clusters = group_by_cluster(&logs);

        let output_tabs = if clusters.is_empty() {
            vec!["No results found".into()]
        } else {
            let mut keys: Vec<u8> = clusters.keys().cloned().collect();
            keys.sort_unstable();
            keys.into_iter().map(|k| format!("Cluster {}", k)).collect()
        };

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
                output_tabs.clone(),
                vec!["Version Info".into(), "License".into()],
            ],
            tab_items_original: vec![
                vec![
                    "General Stats".into(),
                    "Requested Files".into(),
                    "Static Requests".into(),
                    "Not Found URLs".into(),
                    "Visitor Graph".into(),
                ],
                output_tabs,
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
            theme: Theme::Default,
            filter: Filter::new(),
            clusters,
            filtered_cluster_rows: None,
            cluster_backup: None,
            scroll_offset: 0,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        // Minimal ukuran terminal
        const MIN_WIDTH: u16 = 60;
        const MIN_HEIGHT: u16 = 15;
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            let warning = Paragraph::new(format!(
                "Terminal size too small:\nWidth = {} Height = {}\n\nMinimum size:\nWidth = {}  Height = {}",
                area.width, area.height, MIN_WIDTH, MIN_HEIGHT,
            ))
            .alignment(Alignment::Center)
            .style(Style::default().fg(self.theme.unfocused_color()).bold())
            .wrap(ratatui::widgets::Wrap { trim: true });
            f.render_widget(warning, area);
            return;
        }

        let footer_text =
            "Keys: q Quit | ←/→ move focus | ↑/↓ navigate | Enter open detail | r rename(Output) | / search | Esc cancel";

        let width = area.width as usize;
        let lines = (footer_text.len() + width - 1) / width;
        let footer_height = lines as u16 + 2;

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
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

        let label_block = Block::bordered().title("Info");
        let label = Paragraph::new(Line::from(vec![
            Span::styled("Dashboard ", Style::default().fg(self.theme.header_color()).bold()),
            Span::styled("by YourName", Style::default().fg(self.theme.unfocused_color()).italic()),
        ]))
        .block(label_block)
        .centered();
        f.render_widget(label, chunks[0]);

        // search bar aktif
        self.filter.draw_searchbar(f, chunks[1], &self.theme);
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
                style = style.fg(self.theme.focused_color());
                if matches!(self.focus, Focus::Tabs) { style = style.reversed(); }
            } else {
                style = style.fg(self.theme.unfocused_color());
            }
            ListItem::new(Span::styled(item, style))
        }).collect();
        let sidebar = List::new(items).block(Block::default().title("Tabs").borders(Borders::ALL));
        f.render_widget(sidebar, chunks[0]);

        let current_items = &self.tab_items[self.sidebar_index];

        // Output → Detail: tampilkan isi cluster (atau hasil filter cluster)
        if self.sidebar_index == 1 && matches!(self.focus, Focus::Detail) {
            // Pastikan ada item yang terseleksi
            if let Some(cluster_label) = current_items.get(self.content_selection) {
                if let Some(cid) = cluster_label.strip_prefix("Cluster ") {
                    if let Ok(cid_num) = cid.parse::<u8>() {
                        // Pilih sumber data: hasil filter aktif untuk cluster ini, atau raw cluster
                        let logs_to_show: &[ClusteredLog] = if let Some((active_id, rows)) = &self.filtered_cluster_rows {
                            if *active_id == cid_num {
                                rows
                            } else {
                                self.clusters.get(&cid_num).map(|v| &v[..]).unwrap_or(&[])
                            }
                        } else {
                            self.clusters.get(&cid_num).map(|v| &v[..]).unwrap_or(&[])
                        };

                        // Tabel isi log
                        let header = Row::new(vec!["IP", "Method", "URL", "Status", "Size"])
                            .style(Style::default().add_modifier(Modifier::BOLD));
                        let rows: Vec<Row> = logs_to_show.iter().map(|log| {
                            Row::new(vec![
                                Cell::from(log.ip.clone()),
                                Cell::from(log.method.clone()),
                                Cell::from(log.url.clone()),
                                Cell::from(log.status.to_string()),
                                Cell::from(log.size.to_string()),
                            ])
                        }).collect();

                        let table = Table::new(
                            rows,
                            [
                                Constraint::Length(15),
                                Constraint::Length(8),
                                Constraint::Min(25),
                                Constraint::Length(6),
                                Constraint::Length(8),
                            ],
                        )
                        .header(header)
                        .block(Block::default().borders(Borders::ALL).title(cluster_label.to_string()));

                        f.render_widget(table, chunks[1]);
                        return;
                    }
                }
                // Jika tidak bisa parse "Cluster N", tampilkan placeholder aman
                let msg = Paragraph::new("Invalid cluster label")
                    .style(Style::default().fg(self.theme.unfocused_color()))
                    .block(Block::default().borders(Borders::ALL).title("Output"));
                f.render_widget(msg, chunks[1]);
                return;
            } else {
                // Tidak ada item terseleksi (list kosong)
                let msg = Paragraph::new("No clusters to display")
                    .style(Style::default().fg(self.theme.unfocused_color()))
                    .block(Block::default().borders(Borders::ALL).title("Output"));
                f.render_widget(msg, chunks[1]);
                return;
            }
        }

        // Overview → Detail (seperti semula)
        if matches!(self.focus, Focus::Detail) && self.sidebar_index == 0 {
            match self.content_selection {
                0 => self.draw_general_stats(f, chunks[1]),
                1 => self.draw_table_panel(f, chunks[1], "Requested Files", &self.requested_files),
                2 => self.draw_table_panel(f, chunks[1], "Static Requests", &self.static_requests),
                3 => self.draw_table_panel(f, chunks[1], "Not Found URLs", &self.not_found),
                4 => self.draw_graph_table(f, chunks[1]),
                _ => {}
            }
            return;
        }

        // About / License
        if matches!(self.focus, Focus::Detail) && self.sidebar_index == 2 {
            match self.content_selection {
                0 => {
                    let about = Paragraph::new(vec![
                        Line::from("Bisecting Log Analyzer"),
                        Line::from("Version 1.0.0"),
                        Line::from(""),
                        Line::from("A simple log analysis tool with clustering."),
                        Line::from("Author: YourName"),
                        Line::from(""),
                        Line::from("Press ← to go back."),
                    ])
                    .block(Block::default().borders(Borders::ALL).title("Version Info"));
                    f.render_widget(about, chunks[1]);
                }
                1 => {
                    let license = Paragraph::new(vec![
                        Line::from("MIT License"),
                        Line::from(""),
                        Line::from("Copyright (c) 2025 YourName"),
                        Line::from(""),
                        Line::from("Permission is hereby granted, free of charge, to any person obtaining a copy..."),
                        Line::from(""),
                        Line::from("Press ← to go back."),
                    ])
                    .block(Block::default().borders(Borders::ALL).title("License"));
                    f.render_widget(license, chunks[1]);
                }
                _ => {}
            }
            return;
        }

        // Mode list (bukan Detail)
        // --- SCROLLING LOGIC START ---
        let list_area = chunks[1];
        let visible_count = list_area.height.saturating_sub(2) as usize;
        let total_items = current_items.len();

        let scroll_offset = self.scroll_offset.min(total_items.saturating_sub(1));
        let end = (scroll_offset + visible_count).min(total_items);
        let visible_items = &current_items[scroll_offset..end];

        let items: Vec<ListItem> = visible_items.iter().enumerate().map(|(i, s)| {
            let idx = i + scroll_offset;
            let mut style = Style::default();
            if idx == self.content_selection {
                style = style.fg(self.theme.focused_color());
                if matches!(self.focus, Focus::MainList) { style = style.reversed(); }
            } else {
                style = style.fg(self.theme.unfocused_color());
            }
            ListItem::new(Span::styled(s.clone(), style))
        }).collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL)
            .title(self.sidebar_items[self.sidebar_index]));
        f.render_widget(list, list_area);
        // --- SCROLLING LOGIC END ---

        // Popup rename (tetap)
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
                .style(Style::default().fg(self.theme.method_color()))
                .block(Block::default().borders(Borders::all()).title("Rename"));
            f.render_widget(rename_box, popup_h[1]);
        }
    }

    fn draw_general_stats(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(vec![
                Span::styled("Total Requests: ", Style::default().fg(self.theme.label_color())),
                Span::styled("200", Style::default().fg(self.theme.value_color()).bold()),
            ]),
            Line::from(vec![
                Span::styled("Unique Visitors: ", Style::default().fg(self.theme.label_color())),
                Span::styled("50", Style::default().fg(self.theme.value_color()).bold()),
            ]),
            Line::from(vec![
                Span::styled("Valid Requests: ", Style::default().fg(self.theme.label_color())),
                Span::styled("180", Style::default().fg(self.theme.value_color()).bold()),
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
            "Hits","Visitors","Tx","Method","Proto","Data"
        ]).style(Style::default().add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = data.iter().map(|r| {
            Row::new(vec![
                r.hits.to_string(),
                r.visitors.to_string(),
                r.tx_amount.into(),
                r.method.into(),
                r.proto.into(),
                r.data.into(),
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
        .block(Block::default().borders(Borders::ALL).title("Visitor Graph"));
        f.render_widget(table, area);
    }

    fn draw_table_panel(&self, f: &mut Frame, area: Rect, title: &str, rows: &[FileRow]) {
        let header = Row::new(vec![
            "Hits","Visitors","Tx","Method","Proto","Data"
        ]).style(Style::default().add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = rows.iter().map(|r| {
            Row::new(vec![
                r.hits.to_string(),
                r.visitors.to_string(),
                r.tx_amount.into(),
                r.method.into(),
                r.proto.into(),
                r.data.into(),
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
            .style(Style::default().fg(self.theme.unfocused_color()))
            .block(Block::default().borders(Borders::ALL))
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(footer, area);
    }

    // Helper: string gabungan untuk pencarian baris cluster
    fn stringify_log_for_search(&self, log: &ClusteredLog) -> String {
        format!(
            "{} {} {} {} {} {} {}",
            log.ip, log.method, log.url, log.status, log.size, log.protocol, log.user_agent
        ).to_lowercase()
    }

    // Terapkan filter cluster berdasarkan query terkini pada filter bar
    fn apply_cluster_search(&mut self, cid: u8) {
        let source = self.clusters.get(&cid).cloned().unwrap_or_default();
        let q = self.filter.search_input.iter().collect::<String>().to_lowercase();

        // simpan backup penuh saat awal search
        if self.cluster_backup.is_none() {
            self.cluster_backup = Some((cid, source.clone()));
        }

        if q.is_empty() {
            // jika query kosong, kembalikan isi penuh dari backup
            self.filtered_cluster_rows = self.cluster_backup.take();
            return;
        }

        let filtered: Vec<ClusteredLog> = source.into_iter()
            .filter(|log| self.stringify_log_for_search(log).contains(&q))
            .collect();
        self.filtered_cluster_rows = Some((cid, filtered));
    }


    pub fn handle_key(&mut self, key: &KeyEvent, visible_count: usize) {
        // --- Pilih target filter secara kontekstual ---
        // Jika sekarang di Output → Detail pada cluster tertentu, maka search memfilter BARIS LOG cluster.
        if self.sidebar_index == 1 && matches!(self.focus, Focus::Detail) {
            // Untuk mengaktifkan/menavigasi search bar, tetap panggil filter.handle_key
            // dengan daftar string "representasi baris" (tidak dipakai hasilnya, hanya untuk state mesin ketik).
            // Temukan cluster aktif
            let current_label = self.tab_items[1].get(self.content_selection).cloned().unwrap_or_default();
            let active_cid = current_label.strip_prefix("Cluster ").and_then(|s| s.parse::<u8>().ok());

            if let Some(cid) = active_cid {
                // Bangun daftar string baris untuk konsumsi Filter (sekadar aktivasi cursor, dsb.)
                let base_rows = if let Some((aid, rows)) = &self.filtered_cluster_rows {
                    if *aid == cid { rows.clone() } else { self.clusters.get(&cid).cloned().unwrap_or_default() }
                } else {
                    self.clusters.get(&cid).cloned().unwrap_or_default()
                };

                let lines: Vec<String> = base_rows.iter().map(|log| {
                    // ringkas saja agar tidak berat di header; pencarian aslinya pakai stringify_log_for_search
                    format!("{} {} {} {}", log.ip, log.method, log.url, log.status)
                }).collect();

                match self.filter.handle_key(key, &lines) {
                    SearchAction::Update(_) => {
                        self.apply_cluster_search(cid);
                        return;
                    }
                    SearchAction::Exit => {
                        // keluar dari mode search
                        if self.filter.search_input.is_empty() {
                            // jika query kosong, kembalikan isi penuh dari backup
                            if let Some((cid, rows)) = self.cluster_backup.take() {
                                self.filtered_cluster_rows = Some((cid, rows));
                            } else {
                                self.filtered_cluster_rows = None;
                            }
                        }
                        return;
                    }
                    SearchAction::None => {} // lanjut ke navigasi umum
                }
            }
        } else {
            // --- Perilaku filter lama: memfilter daftar tab aktif ---
            match self.filter.handle_key(key, &self.tab_items_original[self.sidebar_index]) {
                SearchAction::Update(filtered) => {
                    self.tab_items[self.sidebar_index] = filtered;
                    if self.content_selection >= self.tab_items[self.sidebar_index].len() {
                        self.content_selection = self.tab_items[self.sidebar_index].len().saturating_sub(1);
                    }
                    return;
                }
                SearchAction::Exit => {
                    // Tidak reset menu di sini! (biarkan hasil filter tetap tampil)
                    self.focus = Focus::MainList;
                    return;
                }
                SearchAction::None => {}
            }
        }

        // --- Navigasi umum ---
        match self.focus {
            Focus::Tabs => match key.code {
                KeyCode::Right => self.focus = Focus::MainList,
                KeyCode::Up if self.sidebar_index > 0 => {
                    self.sidebar_index -= 1;
                    self.content_selection = 0;
                    // keluar dari mode filter cluster jika pindah tab
                    self.filtered_cluster_rows = None;
                }
                KeyCode::Down if self.sidebar_index + 1 < self.sidebar_items.len() => {
                    self.sidebar_index += 1;
                    self.content_selection = 0;
                    self.filtered_cluster_rows = None;
                }
                KeyCode::Char('q') => self.running = false,
                _ => {}
            },
            Focus::MainList => match key.code {
                KeyCode::Left => self.focus = Focus::Tabs,
                KeyCode::Right | KeyCode::Enter => {
                    self.focus = Focus::Detail;
                    // saat masuk detail, reset hasil filter cluster (biar bersih)
                    self.filtered_cluster_rows = None;
                }
                KeyCode::Up => self.scroll_up(visible_count),
                KeyCode::Down => self.scroll_down(visible_count),
                KeyCode::Char('r') if self.sidebar_index == 1 => {
                    self.rename_buffer = self.tab_items[1].get(self.content_selection).cloned().unwrap_or_default();
                    self.focus = Focus::Rename;
                }
                KeyCode::Char('q') => self.running = false,
                _ => {}
            },
            Focus::Detail => match key.code {
                KeyCode::Left => {
                    self.focus = Focus::MainList;
                    // keluar dari detail → matikan filter cluster agar tidak nyangkut
                    self.filtered_cluster_rows = None;
                    self.cluster_backup = None;
                }
                KeyCode::Char('q') => self.running = false,
                _ => {}
            },
            Focus::Rename => match key.code {
                KeyCode::Enter => {
                    if self.sidebar_index == 1 {
                        if let Some(item) = self.tab_items[1].get_mut(self.content_selection) {
                            *item = self.rename_buffer.clone();
                        }
                    }
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

    pub fn scroll_up(&mut self, visible_count: usize) {
        let len = self.tab_items[self.sidebar_index].len();
        if len == 0 { return; }
        if self.content_selection == 0 {
            self.content_selection = len - 1;
            self.scroll_offset = if len > visible_count { len - visible_count } else { 0 };
        } else {
            self.content_selection -= 1;
            if self.content_selection < self.scroll_offset {
                self.scroll_offset = self.content_selection;
            }
        }
    }

    pub fn scroll_down(&mut self, visible_count: usize) {
        let len = self.tab_items[self.sidebar_index].len();
        if len == 0 { return; }
        if self.content_selection + 1 >= len {
            self.content_selection = 0;
            self.scroll_offset = 0;
        } else {
            self.content_selection += 1;
            // Perbaikan: scroll_offset diatur agar selection selalu berada di dalam window
            if self.content_selection >= self.scroll_offset + visible_count {
                self.scroll_offset = self.content_selection - (visible_count - 1);
            }
        }
    }
}
