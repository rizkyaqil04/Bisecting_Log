use crate::terminal_check::{is_too_small, draw_too_small_warning};
use crate::float::{Float, FloatContent};
use crate::hint::Shortcut;
use crate::sort::{SortMenu, SortOrder};
use crate::quit::ConfirmQuit;
use crate::{
    cli::Args,
    data::{ClusterIndex, Table},
    filter::{Filter, SearchAction},
    theme::Theme,
};
use anyhow::Result;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row},
};
use std::{path::PathBuf, time::Duration};

pub enum Mode {
    ClusterList,
    ClusterTable,
}

pub struct App {
    theme: Theme,
    mode: Mode,
    table: Table,
    index: ClusterIndex,
    cluster_sel: usize,
    selected_cluster_id: Option<usize>,
    list_state: ratatui::widgets::ListState,
    table_scroll: usize,
    filter: Filter,
    input_path: PathBuf,
    sort_menu: Option<Float<SortMenu>>,
    current_sort: (String, SortOrder),
    confirm_quit: Option<Float<ConfirmQuit>>,
}

impl App {
    pub fn new(args: Args) -> Result<Self> {
        let (_input_log, csv_path) = args.resolve_paths()?;
        let table = crate::data::read_csv_or_gz(csv_path.as_path())?;
        let index = crate::data::build_cluster_index(&table)?;

        let mut list_state = ratatui::widgets::ListState::default();
        let start_sel = 0usize;
        list_state.select(Some(start_sel));
        Ok(Self {
            theme: Theme::Default,
            mode: Mode::ClusterList,
            table,
            index,
            cluster_sel: 0,
            selected_cluster_id: None,
            list_state,
            table_scroll: 0,
            filter: Filter::default(),
            input_path: csv_path.clone(),
            sort_menu: None,
            current_sort: ("ip".into(), SortOrder::Ascend),
            confirm_quit: None,
        })
    }

    pub fn run(
        &mut self,
        term: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        loop {
            term.draw(|f| self.draw(f))?;
            if !event::poll(Duration::from_millis(50))? {
                continue;
            }
            match event::read()? {
                Event::Key(k) => {
                    if k.kind == KeyEventKind::Release {
                        continue;
                    }
                    if self.handle_key(k.code) == false {
                        break;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        // Tangani konfirmasi keluar (popup float)
        if let Some(ref mut float) = self.confirm_quit {
            let dummy_event = ratatui::crossterm::event::KeyEvent::from(code);
            float.handle_key_event(&dummy_event);

            if float.content.is_finished() {
                let confirmed = float.content.confirmed();
                self.confirm_quit = None;
                if confirmed {
                    return false;
                }
            }
            return true;
        }

        // Kalau tidak ada popup konfirmasi, lanjut ke mode normal
        match self.mode {
            Mode::ClusterList => self.handle_key_list(code),
            Mode::ClusterTable => self.handle_key_table(code),
        }
    }

    fn handle_key_list(&mut self, code: KeyCode) -> bool {
        use KeyCode::*;
        if self.filter.active() {
            match self
                .filter
                .handle_key(&ratatui::crossterm::event::KeyEvent::from(code))
            {
                SearchAction::Exit => {
                    self.filter.deactivate();
                }
                SearchAction::Update => {
                    // setiap kali teks filter berubah, perbarui fokus agar tetap valid
                    let filtered = self.filtered_clusters();
                    if filtered.is_empty() {
                        self.list_state.select(None);
                    } else {
                        let current = self.list_state.selected().unwrap_or(0);
                        let new_sel = current.min(filtered.len().saturating_sub(1));
                        self.list_state.select(Some(new_sel));
                    }
                }
                SearchAction::None => {}
            }
            return true;
        }

        match code {
            Char('q') => {
                self.confirm_quit = Some(Float::new_absolute(
                    Box::new(ConfirmQuit::new()),
                    40,
                    6,
                ));
                return true;
            }
            Char('/') => {
                self.filter.activate();
            }
            Up | Char('k') => self.move_sel_up(),
            Down | Char('j') => self.move_sel_down(),
            Enter | Right | Char('l') => {
                // Simpan ID cluster yang dipilih
                if let Some(idx) = self.list_state.selected() {
                    if let Some((cid, _)) = self.filtered_clusters().get(idx) {
                        self.selected_cluster_id = Some(*cid);
                    }
                }
                self.mode = Mode::ClusterTable;
                self.table_scroll = 0;
            }
            _ => {}
        }
        true
    }

    fn handle_key_table(&mut self, code: KeyCode) -> bool {
        use KeyCode::*;

        // Jika float sort aktif, arahkan semua input ke sana
        if let Some(ref mut float) = self.sort_menu {
            let dummy_event = ratatui::crossterm::event::KeyEvent::from(code);
            let finished = float.handle_key_event(&dummy_event);

            // Jika user tekan ESC, batalkan
            if matches!(code, Esc) {
                self.sort_menu = None;
                return true;
            }

            // Jika user tekan Enter dan sort selesai, terapkan perubahan
            if finished && float.content.is_finished() {
                let col = float.content.columns[float.content.selected_col].clone();
                let ord = float.content.selected_order;
                self.apply_sort(&col, ord);
                self.current_sort = (col, ord);
                self.sort_menu = None;
            }

            return true;
        }

        // Tambahkan blok ini agar filter/search juga aktif di ClusterTable
        if self.filter.active() {
            match self
                .filter
                .handle_key(&ratatui::crossterm::event::KeyEvent::from(code))
            {
                crate::filter::SearchAction::Exit => {
                    self.filter.deactivate();
                }
                crate::filter::SearchAction::Update => {}
                crate::filter::SearchAction::None => {}
            }
            return true;
        }

        // Normal cluster table navigation
        match code {
            Char('q') => {
                self.confirm_quit = Some(Float::new_absolute(
                    Box::new(ConfirmQuit::new()),
                    40,
                    6,
                ));
                return true;
            }
            Left | Char('h') | Esc => {
                self.mode = Mode::ClusterList;
                self.selected_cluster_id = None;
            }
            Down | Char('j') => {
                self.table_scroll = self.table_scroll.saturating_add(1);
            }
            Up | Char('k') => {
                self.table_scroll = self.table_scroll.saturating_sub(1);
            }
            Char('/') => {
                self.filter.activate();
            }
            Char('s') => {
                let (col, ord) = self.current_sort.clone();
                let cols = self.table.headers.clone();
                let default_idx = cols.iter().position(|c| c == &col).unwrap_or(0);
                self.sort_menu = Some(Float::new_absolute(
                    Box::new(SortMenu::new(cols, default_idx, ord)),
                    60,
                    20,
                ));
            }
            _ => {}
        }
        true
    }

    fn move_sel_up(&mut self) {
        let len = self.filtered_clusters().len();
        if len == 0 {
            self.list_state.select(None);
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let next = if i == 0 { len - 1 } else { i - 1 };
        self.list_state.select(Some(next));
        self.cluster_sel = next;
    }

    fn move_sel_down(&mut self) {
        let len = self.filtered_clusters().len();
        if len == 0 {
            self.list_state.select(None);
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let next = if i >= len - 1 { 0 } else { i + 1 };
        self.list_state.select(Some(next));
        self.cluster_sel = next;
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        if is_too_small(area) {
            draw_too_small_warning(f, area);
            return;
        }

        // Hitung tinggi dinamis berdasarkan teks hint yang sebenarnya
        let (_title, shortcuts) = self.get_current_shortcuts();
        let lines = crate::hint::create_shortcut_list(shortcuts, area.width);
        let actual_hint_height = lines.len() as u16 + 2; // +2 buat padding border

        // Batas minimum dan maksimum biar tidak terlalu kecil/tinggi
        let hint_height = actual_hint_height.clamp(3, 20);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),           // header
                Constraint::Min(1),              // body
                Constraint::Length(hint_height), // hint (dinamis)
            ])
            .split(area);

        // header
        let title = Paragraph::new(format!(
            "BKM Viewer  |  File: {}  |  Mode: {:?}",
            self.input_path.display(),
            self.mode_name()
        ))
        .style(Style::default().fg(self.theme.title_color())); // gunakan theme

        f.render_widget(title, chunks[0]);

        // body
        let body = match self.mode {
            Mode::ClusterList => Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
                .split(chunks[1]),
            Mode::ClusterTable => std::rc::Rc::from([chunks[1]]),
        };

        match self.mode {
            Mode::ClusterList => {
                let left_top = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
                    .split(body[0]);
                self.filter.draw(f, left_top[0]);
                self.draw_cluster_list(f, left_top[1]);
                self.draw_preview(f, body[1]);
            }
            Mode::ClusterTable => {
                self.draw_table(f, body[0]);
            }
        }

        // HINT WINDOW
        self.draw_hint(f, chunks[2]);

        if let Some(ref mut float) = self.confirm_quit {
            float.draw(f, f.area(), &self.theme);
        }
    }

    fn mode_name(&self) -> &str {
        match self.mode {
            Mode::ClusterList => "ClusterList",
            Mode::ClusterTable => "ClusterTable",
        }
    }

    fn filtered_clusters(&self) -> Vec<(usize, usize)> {
        let query = self.filter.parsed_query(); // ambil beberapa ekspresi terstruktur
        let term = self.filter.term().to_lowercase();

        let matches_row = |row: &Vec<String>| -> bool {
            if let Some(ref q) = query {
                // Semua kondisi (expr) harus cocok => AND logic
                q.exprs.iter().all(|expr| {
                    if expr.key.is_empty() {
                        // jika tanpa key (misal: cuma ketik "admin")
                        row.iter().any(|v| v.to_lowercase().contains(&expr.value.to_lowercase()))
                    } else if let Some(idx) = self.table.headers.iter().position(|h| h == &expr.key) {
                        let val = &row[idx].to_lowercase();
                        let val_num = val.parse::<f64>().ok();
                        let target_num = expr.value.parse::<f64>().ok();

                        match expr.op {
                            crate::filter::SearchOp::Eq => val.contains(&expr.value.to_lowercase()),
                            crate::filter::SearchOp::EqExact => val == &expr.value.to_lowercase(),
                            crate::filter::SearchOp::NotEq => !val.contains(&expr.value.to_lowercase()),
                            crate::filter::SearchOp::Gt => val_num.zip(target_num).map_or(false, |(a, b)| a > b),
                            crate::filter::SearchOp::Lt => val_num.zip(target_num).map_or(false, |(a, b)| a < b),
                            crate::filter::SearchOp::Ge => val_num.zip(target_num).map_or(false, |(a, b)| a >= b),
                            crate::filter::SearchOp::Le => val_num.zip(target_num).map_or(false, |(a, b)| a <= b),
                            crate::filter::SearchOp::Contains => val.contains(&expr.value.to_lowercase()),
                        }
                    } else {
                        false
                    }
                })
            } else if term.is_empty() {
                true
            } else {
                // fallback: simple contains search
                row.iter().any(|v| v.to_lowercase().contains(&term))
            }
        };

        let mut out = Vec::new();
        match self.mode {
            Mode::ClusterList => {
                for c in &self.index.clusters {
                    let count = c.rows_idx
                        .iter()
                        .filter(|&&ri| matches_row(&self.table.rows[ri]))
                        .count();
                    if count > 0 {
                        out.push((c.id, count));
                    }
                }
            }
            Mode::ClusterTable => {
                if let Some(cid) = self.selected_cluster_id {
                    if let Some(c) = self.index.clusters.iter().find(|c| c.id == cid) {
                        let count = c.rows_idx
                            .iter()
                            .filter(|&&ri| matches_row(&self.table.rows[ri]))
                            .count();
                        if count > 0 {
                            out.push((c.id, count));
                        }
                    }
                }
            }
        }

        out
    }


    fn draw_cluster_list(&mut self, f: &mut Frame, area: Rect) {
        let items = self
            .filtered_clusters()
            .iter()
            .map(|(id, n)| {
                let s = format!("Cluster {} ({} entries)", id, n);
                ListItem::new(s).style(Style::default().fg(self.theme.cluster_color()))
            })
            .collect::<Vec<_>>();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Clusters ");
        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .fg(self.theme.selection_fg())
                .bg(self.theme.selection_bg())
                .add_modifier(Modifier::BOLD),
        );

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_preview(&self, f: &mut Frame, area: Rect) {
        // tampilkan beberapa baris dari cluster terpilih, TERFILTER oleh search
        let filtered = self.filtered_clusters();
        let idx = self.list_state.selected().unwrap_or(0);
        let mut lines = Vec::new();

        if !filtered.is_empty() {
            let cluster_id = filtered[idx].0;
            if let Some(c) = self.index.clusters.iter().find(|c| c.id == cluster_id) {
                // Judul preview
                lines.push(
                    Line::from(format!("Preview Cluster {}", cluster_id)).style(
                        Style::default()
                            .fg(self.theme.preview_color())
                            .add_modifier(Modifier::BOLD),
                    ),
                );

                // Gunakan parsed_query agar mendukung advanced search (key op value)
                let query = self.filter.parsed_query();
                let term = self.filter.term().to_lowercase();

                let filtered_rows = if term.is_empty() && query.is_none() {
                    c.rows_idx.iter().cloned().collect::<Vec<_>>()
                } else {
                    c.rows_idx
                        .iter()
                        .cloned()
                        .filter(|&ri| {
                            let row = &self.table.rows[ri];
                            if let Some(ref q) = query {
                                // Semua ekspresi harus cocok
                                q.exprs.iter().all(|expr| {
                                    if expr.key.is_empty() {
                                        row.iter().any(|v| v.to_lowercase().contains(&expr.value.to_lowercase()))
                                    } else if let Some(idx) = self.table.headers.iter().position(|h| h == &expr.key) {
                                        let val = &row[idx].to_lowercase();
                                        let val_num = val.parse::<f64>().ok();
                                        let target_num = expr.value.parse::<f64>().ok();
                                        match expr.op {
                                            crate::filter::SearchOp::Eq => val.contains(&expr.value.to_lowercase()),
                                            crate::filter::SearchOp::EqExact => val == &expr.value.to_lowercase(),
                                            crate::filter::SearchOp::NotEq => !val.contains(&expr.value.to_lowercase()),
                                            crate::filter::SearchOp::Gt => val_num.zip(target_num).map_or(false, |(a, b)| a > b),
                                            crate::filter::SearchOp::Lt => val_num.zip(target_num).map_or(false, |(a, b)| a < b),
                                            crate::filter::SearchOp::Ge => val_num.zip(target_num).map_or(false, |(a, b)| a >= b),
                                            crate::filter::SearchOp::Le => val_num.zip(target_num).map_or(false, |(a, b)| a <= b),
                                            crate::filter::SearchOp::Contains => val.contains(&expr.value.to_lowercase()),
                                        }
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                row.iter().any(|v| v.to_lowercase().contains(&term))
                            }
                        })
                        .collect::<Vec<_>>()
                };

                // Tambahkan pesan jika hasil kosong
                if filtered_rows.is_empty() {
                    lines.push(
                        Line::from("No matching entries found.")
                            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
                    );
                }

                // Render maksimal 10 baris hasil
                for &ri in filtered_rows.iter().take(10) {
                    let row = &self.table.rows[ri];
                    // bentuk ringkas: ip method url status size
                    let ip = self.pick("ip", row).unwrap_or_default();
                    let method = self.pick("method", row).unwrap_or_default();
                    let url = self.pick("url", row).unwrap_or_default();
                    let status = self.pick("status", row).unwrap_or_default();
                    let size = self.pick("size", row).unwrap_or_default();
                    lines.push(Line::from(format!("{ip} {method} {url} {status} {size}")));
                }
            }
        }

        let p = Paragraph::new(Text::from(lines)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Preview ")
                .border_type(ratatui::widgets::BorderType::Rounded),
        );
        f.render_widget(p, area);
    }

    fn draw_table(&mut self, f: &mut Frame, area: Rect) {
        let filtered = self.filtered_clusters();
        let idx = 0; // Selalu 0 karena hanya satu cluster di ClusterTable

        // === Split area jadi dua bagian besar: Header bar dan Tabel ===
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header bar (search + sort info)
                Constraint::Min(5),    // Table data
            ])
            .split(area);

        // === HEADER BAR (1 baris, dua kolom) ===
        let header_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70), // kiri: search
                Constraint::Percentage(30), // kanan: sort info
            ])
            .split(layout[0]);

        // kiri: SEARCH BAR
        self.filter.draw(f, header_chunks[0]);

        // kanan: INFO SORT BY + ORDER
        let sort_text = format!(
            "Sort by: {}   |   Order: {}",
            self.current_sort.0,
            match self.current_sort.1 {
                SortOrder::Ascend => "Ascend",
                SortOrder::Descend => "Descend",
            }
        );

        let sort_block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Sorting ")
            .title_alignment(Alignment::Left);

        let sort_para = Paragraph::new(sort_text)
            .style(Style::default())
            .alignment(Alignment::Center)
            .block(sort_block);

        f.render_widget(sort_para, header_chunks[1]);

        if let Some((cluster_id, _count)) = filtered.get(idx).copied() {
            let Some(c) = self.index.clusters.iter().find(|c| c.id == cluster_id) else {
                let p = Paragraph::new("Tidak ada data").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Info ")
                        .border_type(ratatui::widgets::BorderType::Rounded),
                );
                f.render_widget(p, layout[1]);
                return;
            };

            let query = self.filter.parsed_query();
            let term = self.filter.term().to_lowercase();

            let filtered_rows_idx: Vec<_> = c
                .rows_idx
                .iter()
                .cloned()
                .filter(|&ri| {
                    let row = &self.table.rows[ri];
                    if let Some(ref q) = query {
                        // Semua ekspresi dalam query harus cocok (AND logic)
                        q.exprs.iter().all(|expr| {
                            if expr.key.is_empty() {
                                row.iter().any(|v| v.to_lowercase().contains(&expr.value.to_lowercase()))
                            } else if let Some(idx) = self.table.headers.iter().position(|h| h == &expr.key) {
                                let val = &row[idx].to_lowercase();
                                let val_num = val.parse::<f64>().ok();
                                let target_num = expr.value.parse::<f64>().ok();
                                match expr.op {
                                    crate::filter::SearchOp::Eq => val.contains(&expr.value.to_lowercase()),
                                    crate::filter::SearchOp::EqExact => val == &expr.value.to_lowercase(),
                                    crate::filter::SearchOp::NotEq => !val.contains(&expr.value.to_lowercase()),
                                    crate::filter::SearchOp::Gt => val_num.zip(target_num).map_or(false, |(a, b)| a > b),
                                    crate::filter::SearchOp::Lt => val_num.zip(target_num).map_or(false, |(a, b)| a < b),
                                    crate::filter::SearchOp::Ge => val_num.zip(target_num).map_or(false, |(a, b)| a >= b),
                                    crate::filter::SearchOp::Le => val_num.zip(target_num).map_or(false, |(a, b)| a <= b),
                                    crate::filter::SearchOp::Contains => val.contains(&expr.value.to_lowercase()),
                                }
                            } else {
                                false
                            }
                        })
                    } else if term.is_empty() {
                        true
                    } else {
                        row.iter().any(|v| v.to_lowercase().contains(&term))
                    }
                })
                .collect();

            // HEADER
            let header = Row::new(self.table.headers.iter().map(|h| {
                Cell::from(h.as_str()).style(
                    Style::default()
                        .fg(self.theme.table_header())
                        .bg(self.theme.border_color())
                        .add_modifier(Modifier::BOLD),
                )
            }));

            let mut rows = Vec::new();
            // TABLE ROWS
            for (i, &ri) in filtered_rows_idx
                .iter()
                .skip(self.table_scroll)
                .take(layout[1].height.saturating_sub(3) as usize)
                .enumerate()
            {
                let r = &self.table.rows[ri];
                let bg = if i % 2 == 0 {
                    self.theme.table_row_even()
                } else {
                    self.theme.table_row_odd()
                };
                rows.push(Row::new(r.iter().map(|v| {
                    Cell::from(v.as_str()).style(
                        Style::default()
                            // .fg(self.theme.table_text())
                            .bg(bg),
                    )
                })));
            }

            let table = ratatui::widgets::Table::new(rows, self.auto_widths(area.width))
                .header(header)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(format!(
                            " Cluster {} - {} rows ",
                            cluster_id,
                            filtered_rows_idx.len()
                        )),
                )
                .row_highlight_style(
                    Style::default()
                        .bg(self.theme.selection_bg())
                        .fg(self.theme.selection_fg())
                        .add_modifier(Modifier::BOLD),
                );

            f.render_widget(table, layout[1]);

            // === FLOAT SORT MENU (jika aktif) ===
            if let Some(ref mut float) = self.sort_menu {
                float.draw(f, area, &self.theme);
                if float.content.is_finished() {
                    if float.content.cancelled {
                        self.sort_menu = None;
                    } else {
                        let col = float.content.columns[float.content.selected_col].clone();
                        let ord = float.content.selected_order;
                        self.apply_sort(&col, ord);
                        self.current_sort = (col, ord);
                        self.sort_menu = None;
                    }
                }
            }
        } else {
            // TIDAK ADA DATA: render pesan di area tabel saja
            let p = Paragraph::new("Tidak ada data")
                .style(Style::default().fg(self.theme.info_color()))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Info ")
                        .border_type(ratatui::widgets::BorderType::Rounded),
                );
            f.render_widget(p, layout[1]);
        }
    }

    fn auto_widths(&self, total: u16) -> Vec<Constraint> {
        // bagi rata sederhana, nanti bisa dibuat adaptif
        let cols = self.table.headers.len().max(1) as u16;
        let w = (total - 4).saturating_div(cols);
        (0..cols).map(|_| Constraint::Length(w)).collect()
    }

    fn pick(&self, col: &str, row: &Vec<String>) -> Option<String> {
        self.table
            .column_index(col)
            .and_then(|i| row.get(i).cloned())
    }

    fn apply_sort(&mut self, col: &str, order: SortOrder) {
        if let Some(idx) = self.table.column_index(col) {
            for c in &mut self.index.clusters {
                c.rows_idx.sort_by(|&a, &b| {
                    let va = &self.table.rows[a][idx];
                    let vb = &self.table.rows[b][idx];

                    // Coba parse ke f64
                    let va_num = va.parse::<f64>();
                    let vb_num = vb.parse::<f64>();

                    let ord = if va_num.is_ok() && vb_num.is_ok() {
                        va_num.unwrap().partial_cmp(&vb_num.unwrap()).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        va.cmp(vb)
                    };

                    if order == SortOrder::Ascend {
                        ord
                    } else {
                        ord.reverse()
                    }
                });
            }
        }
    }

    fn draw_hint(&self, f: &mut Frame, area: Rect) {
        let (_title, shortcuts) = self.get_current_shortcuts();
        let lines = crate::hint::create_shortcut_list(shortcuts, area.width);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(format!(" {} Shortcuts ", _title));

        let para = Paragraph::new(lines.to_vec())
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false });

        f.render_widget(para, area);
    }

    fn get_current_shortcuts(&self) -> (&str, Box<[crate::hint::Shortcut]>) {
        if let Some(ref float) = self.sort_menu {
            return float.get_shortcut_list();
        }
        if let Some(ref float) = self.confirm_quit {
            return float.get_shortcut_list();
        }
        // Jika filter/search aktif
        if self.filter.active() {
            (
                "Search",
                crate::shortcuts!(
                    ("Exit search", ["Esc", "Enter"]),
                    ("Move cursor", ["←", "→"]),
                    ("Delete char", ["Backspace"]),
                    ("Input", ["a-z", "0-9", "etc"]),
                ),
            )
        } else {
            match self.mode {
                Mode::ClusterList => (
                    "Cluster List",
                    crate::shortcuts!(
                        ("Move", ["j", "k", "↑", "↓"]),
                        ("Select cluster", ["Enter", "l", "→"]),
                        ("Search", ["/"]),
                        ("Quit", ["q"]),
                    ),
                ),
                Mode::ClusterTable => (
                    "Cluster Table",
                    crate::shortcuts!(
                        ("Scroll", ["j", "k", "↑", "↓"]),
                        ("Sort", ["s"]),
                        ("Search", ["/"]),
                        ("Back", ["h", "←", "Esc"]),
                        ("Quit", ["q"]),
                    ),
                ),
            }
        }
    }
}
