use crate::{
    cli::Args,
    data::{ClusterIndex, Table},
    detail::DataDetail,
    filter::{Filter, SearchAction},
    float::{Float, FloatContent},
    hint::Shortcut,
    loading::LoadingFloat,
    quit::ConfirmQuit,
    sort::{SortMenu, SortOrder},
    terminal_check::{draw_too_small_warning, is_too_small},
    theme::Theme,
};
use anyhow::Result;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row},
};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::{path::PathBuf, time::Duration};

const TABLE_PAGE_SIZE: usize = 50;

pub enum Mode {
    ClusterList,
    ClusterTable,
}

// LoadingFloat moved to `float.rs` to centralize floating popup implementations

// Message from background filter worker.
enum FilterResult {
    ClusterRows(Vec<usize>),
    Both(Vec<(usize, usize)>, Vec<usize>),
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
    table_page: usize,
    table_view_offset: usize,
    // cache filtered rows for the currently selected cluster to avoid rescanning
    cached_cluster_id: Option<usize>,
    cached_filtered_rows: Vec<usize>,
    filter: Filter,
    // incremental scan cache for ClusterList filtering: (cluster_id, matched_count, next_row_index)
    cluster_list_cache_key: Option<String>,
    cluster_list_cache: Vec<(usize, usize, usize)>,
    input_path: PathBuf,
    /// Last committed query (set when user presses Enter)
    committed_query: Option<crate::filter::SearchQuery>,
    /// Last committed plain term (set when user presses Enter)
    committed_term: String,
    sort_menu: Option<Float<SortMenu>>,
    current_sort: (String, SortOrder),
    confirm_quit: Option<Float<ConfirmQuit>>,
    detail_float: Option<Float<DataDetail>>,
    /// Loading float shown while background filtering is running
    loading_float: Option<Float<LoadingFloat>>,
    /// Receiver for background filter results
    filter_result_rx: Option<Receiver<FilterResult>>,
}

impl App {
    /// Construct App from pre-loaded `Table` and `ClusterIndex`.
    /// Useful when the TUI wants to display a loading popup while reading the input
    /// file in a background thread and then build the App from the loaded data.
    pub fn from_parts(
        args: Args,
        table: crate::data::Table,
        index: crate::data::ClusterIndex,
    ) -> Result<Self> {
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
            table_page: 0,
            table_view_offset: 0,
            cached_cluster_id: None,
            cached_filtered_rows: Vec::new(),
            detail_float: None,
            cluster_list_cache_key: None,
            cluster_list_cache: Vec::new(),
            filter: Filter::default(),
            committed_query: None,
            committed_term: String::new(),
            input_path: args.resolve_paths()?.1,
            sort_menu: None,
            current_sort: ("ip".into(), SortOrder::Ascend),
            confirm_quit: None,
            loading_float: None,
            filter_result_rx: None,
        })
    }

    pub fn run(
        &mut self,
        term: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        loop {
            // Check for background filter results and apply them before rendering
            if let Some(rx) = &self.filter_result_rx {
                match rx.try_recv() {
                    Ok(msg) => {
                        match msg {
                            FilterResult::Both(counts, rows) => {
                                // Update cluster_list_cache with full counts and mark scan complete
                                self.cluster_list_cache.clear();
                                for (cid, cnt) in counts {
                                    // find total rows for this cluster to mark scanning done
                                    let total = self
                                        .index
                                        .clusters
                                        .iter()
                                        .find(|c| c.id == cid)
                                        .map(|c| c.rows_idx.len())
                                        .unwrap_or(0usize);
                                    self.cluster_list_cache.push((cid, cnt, total));
                                }
                                // update cached rows for current cluster and mark which cluster they belong to
                                self.cached_filtered_rows = rows;
                                self.cached_cluster_id = self.selected_cluster_id;
                                // clear loading indicator
                                self.loading_float = None;
                                self.filter_result_rx = None;
                            }
                            FilterResult::ClusterRows(rows) => {
                                self.cached_filtered_rows = rows;
                                self.cached_cluster_id = self.selected_cluster_id;
                                self.loading_float = None;
                                self.filter_result_rx = None;
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(_) => {
                        // disconnected or other error: clear receiver and loading
                        self.filter_result_rx = None;
                        self.loading_float = None;
                    }
                }
            }
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
        // Floating windows take priority: detail float, then confirm quit.
        if let Some(ref mut float) = self.detail_float {
            let dummy_event = ratatui::crossterm::event::KeyEvent::from(code);
            let finished = float.handle_key_event(&dummy_event);
            if finished && float.content.is_finished() {
                self.detail_float = None;
            }
            return true;
        }

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
                    // Commit the current query/term and run the background worker to
                    // recompute cluster counts and (optionally) filtered rows. Do NOT
                    // switch to ClusterTable here; remain in ClusterList and update
                    // the visible cluster list when the worker finishes.
                    let parsed = self.filter.parsed_query();
                    let term = self.filter.term();
                    self.committed_query = parsed.clone();
                    self.committed_term = term.clone();

                    // Deactivate search UI (freeze current screen) and show loading float
                    self.filter.deactivate();
                    self.loading_float = Some(Float::new_absolute(
                        Box::new(LoadingFloat::new("Loading data...")),
                        40,
                        3,
                    ));

                    // Prepare background worker inputs
                    let table_clone = self.table.clone();
                    let index_clone = self.index.clone();
                    let headers_clone = self.table.headers.clone();

                    // determine currently-selected cluster id in the visible (filtered)
                    // cluster list (if any) using the just-committed filter
                    let sel_cluster_id = self
                        .list_state
                        .selected()
                        .and_then(|idx| self.filtered_clusters().get(idx).map(|(cid, _)| *cid));

                    let (tx, rx) = channel();
                    self.filter_result_rx = Some(rx);

                    // Move parsed/term into worker
                    let parsed_move = parsed.clone();
                    let term_move = term.clone();

                    thread::spawn(move || {
                        let matches_row = |row: &Vec<String>,
                                           parsed: &Option<crate::filter::SearchQuery>,
                                           term: &str|
                         -> bool {
                            if let Some(q) = parsed {
                                q.exprs.iter().all(|expr| {
                                    if expr.key.is_empty() {
                                        row.iter().any(|v| {
                                            v.to_lowercase().contains(&expr.value.to_lowercase())
                                        })
                                    } else if let Some(idx) = headers_clone
                                        .iter()
                                        .position(|h| h.eq_ignore_ascii_case(&expr.key))
                                    {
                                        let val = &row[idx].to_lowercase();
                                        let val_num = val.parse::<f64>().ok();
                                        let target_num = expr.value.parse::<f64>().ok();
                                        match expr.op {
                                            crate::filter::SearchOp::Eq => {
                                                val.contains(&expr.value.to_lowercase())
                                            }
                                            crate::filter::SearchOp::EqExact => {
                                                val == &expr.value.to_lowercase()
                                            }
                                            crate::filter::SearchOp::NotEq => {
                                                !val.contains(&expr.value.to_lowercase())
                                            }
                                            crate::filter::SearchOp::Gt => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a > b),
                                            crate::filter::SearchOp::Lt => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a < b),
                                            crate::filter::SearchOp::Ge => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a >= b),
                                            crate::filter::SearchOp::Le => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a <= b),
                                            crate::filter::SearchOp::Contains => {
                                                val.contains(&expr.value.to_lowercase())
                                            }
                                        }
                                    } else {
                                        false
                                    }
                                })
                            } else if term.trim().is_empty() {
                                true
                            } else {
                                row.iter()
                                    .any(|v| v.to_lowercase().contains(&term.to_lowercase()))
                            }
                        };

                        // helper to compute counts per cluster
                        let mut counts: Vec<(usize, usize)> =
                            Vec::with_capacity(index_clone.clusters.len());
                        let mut table_local = table_clone.clone();
                        for c in &index_clone.clusters {
                            let mut matched = 0usize;
                            for &ri in &c.rows_idx {
                                if let Ok(r) = table_local.get_row(ri) {
                                    if matches_row(&r, &parsed_move, &term_move) {
                                        matched += 1;
                                    }
                                }
                            }
                            counts.push((c.id, matched));
                        }

                        // Optionally compute filtered rows for selected cluster
                        let mut rows: Vec<usize> = Vec::new();
                        if let Some(sid) = sel_cluster_id {
                            if let Some(c) = index_clone.clusters.iter().find(|cc| cc.id == sid) {
                                for &ri in &c.rows_idx {
                                    if let Ok(r) = table_local.get_row(ri) {
                                        if matches_row(&r, &parsed_move, &term_move) {
                                            rows.push(ri);
                                        }
                                    }
                                }
                            }
                        }

                        let _ = tx.send(FilterResult::Both(counts, rows));
                    });
                }
                SearchAction::Update => {
                    // Do NOT run live filtering on every keystroke. Leave selection unchanged
                    // until the query is committed with Enter. This freezes the UI while
                    // the user is typing.
                }
                SearchAction::None => {}
            }
            return true;
        }

        match code {
            Char('q') => {
                self.confirm_quit = Some(Float::new_absolute(Box::new(ConfirmQuit::new()), 40, 6));
                return true;
            }
            Char('/') => {
                self.filter.activate();
            }
            Up | Char('k') => self.move_sel_up(),
            Down | Char('j') => self.move_sel_down(),
            Enter | Right | Char('l') => {
                // When user presses Enter while in search, run the query in background
                // and show a Loading float. Do not perform live updates per keystroke.
                // Save current search term / parsed query and spawn worker.
                // Commit the current query and term, then run background worker
                let parsed = self.filter.parsed_query();
                let term = self.filter.term();
                // save as committed query/term so UI is frozen while worker runs
                self.committed_query = parsed.clone();
                self.committed_term = term.clone();

                // Deactivate search UI (freeze current screen) and show loading float
                self.filter.deactivate();
                self.loading_float = Some(Float::new_absolute(
                    Box::new(LoadingFloat::new("Loading data...")),
                    40,
                    3,
                ));

                // Prepare background worker inputs
                let table_clone = self.table.clone();
                let index_clone = self.index.clone();
                let headers_clone = self.table.headers.clone();
                // determine currently-selected cluster id in the visible (filtered) cluster list (if any)
                let sel_cluster_id = self
                    .list_state
                    .selected()
                    .and_then(|idx| self.filtered_clusters().get(idx).map(|(cid, _)| *cid));

                let (tx, rx) = channel();
                self.filter_result_rx = Some(rx);

                // Spawn background thread to compute cluster counts and optionally rows for selected cluster
                thread::spawn(move || {
                    // helper to test a row against parsed query/term
                    let matches_row = |row: &Vec<String>,
                                       parsed: &Option<crate::filter::SearchQuery>,
                                       term: &str|
                     -> bool {
                        if let Some(q) = parsed {
                            q.exprs.iter().all(|expr| {
                                if expr.key.is_empty() {
                                    row.iter().any(|v| {
                                        v.to_lowercase().contains(&expr.value.to_lowercase())
                                    })
                                } else if let Some(idx) = headers_clone
                                    .iter()
                                    .position(|h| h.eq_ignore_ascii_case(&expr.key))
                                {
                                    let val = &row[idx].to_lowercase();
                                    let val_num = val.parse::<f64>().ok();
                                    let target_num = expr.value.parse::<f64>().ok();
                                    match expr.op {
                                        crate::filter::SearchOp::Eq => {
                                            val.contains(&expr.value.to_lowercase())
                                        }
                                        crate::filter::SearchOp::EqExact => {
                                            val == &expr.value.to_lowercase()
                                        }
                                        crate::filter::SearchOp::NotEq => {
                                            !val.contains(&expr.value.to_lowercase())
                                        }
                                        crate::filter::SearchOp::Gt => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a > b)
                                        }
                                        crate::filter::SearchOp::Lt => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a < b)
                                        }
                                        crate::filter::SearchOp::Ge => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a >= b)
                                        }
                                        crate::filter::SearchOp::Le => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a <= b)
                                        }
                                        crate::filter::SearchOp::Contains => {
                                            val.contains(&expr.value.to_lowercase())
                                        }
                                    }
                                } else {
                                    false
                                }
                            })
                        } else if term.trim().is_empty() {
                            true
                        } else {
                            row.iter()
                                .any(|v| v.to_lowercase().contains(&term.to_lowercase()))
                        }
                    };

                    // Compute counts per cluster (use local table instance for caching)
                    let mut counts: Vec<(usize, usize)> =
                        Vec::with_capacity(index_clone.clusters.len());
                    let mut table_local = table_clone.clone();
                    for c in &index_clone.clusters {
                        let mut matched = 0usize;
                        for &ri in &c.rows_idx {
                            if let Ok(r) = table_local.get_row(ri) {
                                if matches_row(&r, &parsed, &term) {
                                    matched += 1;
                                }
                            }
                        }
                        counts.push((c.id, matched));
                    }

                    // Optionally compute filtered rows for selected cluster
                    let mut rows: Vec<usize> = Vec::new();
                    if let Some(sid) = sel_cluster_id {
                        if let Some(c) = index_clone.clusters.iter().find(|cc| cc.id == sid) {
                            for &ri in &c.rows_idx {
                                if let Ok(r) = table_local.get_row(ri) {
                                    if matches_row(&r, &parsed, &term) {
                                        rows.push(ri);
                                    }
                                }
                            }
                        }
                    }

                    // send result back
                    let _ = tx.send(FilterResult::Both(counts, rows));
                });

                // switch to table view for selected cluster if user pressed Right/Enter to open
                if let Some(idx) = self.list_state.selected() {
                    if let Some((cid, _)) = self.filtered_clusters().get(idx) {
                        self.selected_cluster_id = Some(*cid);
                    }
                }
                self.mode = Mode::ClusterTable;
                self.table_scroll = 0;
                self.table_page = 0;
                self.table_view_offset = 0;
                // Invalidate cached filtered rows — they'll be replaced when background work completes
                self.cached_cluster_id = None;
                self.cached_filtered_rows.clear();
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
                    // Run query in background (similar to cluster-list flow)
                    // Commit the current query and term, then run background worker
                    let parsed = self.filter.parsed_query();
                    let term = self.filter.term();
                    self.committed_query = parsed.clone();
                    self.committed_term = term.clone();
                    self.filter.deactivate();
                    self.loading_float = Some(Float::new_absolute(
                        Box::new(LoadingFloat::new("Loading data...")),
                        40,
                        3,
                    ));

                    let table_clone = self.table.clone();
                    let index_clone = self.index.clone();
                    let headers_clone = self.table.headers.clone();
                    // determine currently-selected cluster id in the cluster list (if any)
                    let sel_cluster_id = self
                        .selected_cluster_id
                        .or(self.list_state.selected().and_then(|idx| {
                            self.filtered_clusters().get(idx).map(|(cid, _)| *cid)
                        }));

                    let (tx, rx) = channel();
                    self.filter_result_rx = Some(rx);

                    thread::spawn(move || {
                        let matches_row = |row: &Vec<String>,
                                           parsed: &Option<crate::filter::SearchQuery>,
                                           term: &str|
                         -> bool {
                            if let Some(q) = parsed {
                                q.exprs.iter().all(|expr| {
                                    if expr.key.is_empty() {
                                        row.iter().any(|v| {
                                            v.to_lowercase().contains(&expr.value.to_lowercase())
                                        })
                                    } else if let Some(idx) = headers_clone
                                        .iter()
                                        .position(|h| h.eq_ignore_ascii_case(&expr.key))
                                    {
                                        let val = &row[idx].to_lowercase();
                                        let val_num = val.parse::<f64>().ok();
                                        let target_num = expr.value.parse::<f64>().ok();
                                        match expr.op {
                                            crate::filter::SearchOp::Eq => {
                                                val.contains(&expr.value.to_lowercase())
                                            }
                                            crate::filter::SearchOp::EqExact => {
                                                val == &expr.value.to_lowercase()
                                            }
                                            crate::filter::SearchOp::NotEq => {
                                                !val.contains(&expr.value.to_lowercase())
                                            }
                                            crate::filter::SearchOp::Gt => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a > b),
                                            crate::filter::SearchOp::Lt => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a < b),
                                            crate::filter::SearchOp::Ge => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a >= b),
                                            crate::filter::SearchOp::Le => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a <= b),
                                            crate::filter::SearchOp::Contains => {
                                                val.contains(&expr.value.to_lowercase())
                                            }
                                        }
                                    } else {
                                        false
                                    }
                                })
                            } else if term.trim().is_empty() {
                                true
                            } else {
                                row.iter()
                                    .any(|v| v.to_lowercase().contains(&term.to_lowercase()))
                            }
                        };

                        let mut rows: Vec<usize> = Vec::new();
                        if let Some(sid) = sel_cluster_id {
                            if let Some(c) = index_clone.clusters.iter().find(|cc| cc.id == sid) {
                                let mut table_local = table_clone.clone();
                                for &ri in &c.rows_idx {
                                    if let Ok(r) = table_local.get_row(ri) {
                                        if matches_row(&r, &parsed, &term) {
                                            rows.push(ri);
                                        }
                                    }
                                }
                            }
                        }

                        let _ = tx.send(FilterResult::ClusterRows(rows));
                    });
                }
                crate::filter::SearchAction::Update => {
                    // Do NOT invalidate or re-run query while the user is typing.
                    // The committed results will be updated when Enter is pressed.
                }
                crate::filter::SearchAction::None => {}
            }
            return true;
        }

        // Normal cluster table navigation
        match code {
            // Down/Up move the focus within the current page; clamp by current page length
            Down | Char('j') => {
                // Determine current page length and handle moving past page bottom
                let mut page_len = TABLE_PAGE_SIZE;
                if let Some(cid) = self.selected_cluster_id {
                    if self.cached_cluster_id == Some(cid) {
                        let total_rows = self.cached_filtered_rows.len();
                        let total_pages = if total_rows == 0 {
                            1
                        } else {
                            (total_rows + TABLE_PAGE_SIZE - 1) / TABLE_PAGE_SIZE
                        };
                        let page = if self.table_page >= total_pages {
                            total_pages.saturating_sub(1)
                        } else {
                            self.table_page
                        };
                        let page_start = page.saturating_mul(TABLE_PAGE_SIZE);
                        let page_end = (page_start + TABLE_PAGE_SIZE).min(total_rows);
                        page_len = page_end.saturating_sub(page_start);
                        if page_len == 0 {
                            page_len = 1;
                        }
                    }
                }

                // If not at bottom of current page, move cursor down within page.
                // Do NOT auto-advance to next page when hitting the bottom —
                // page navigation is controlled by Left/Right keys only.
                if self.table_scroll + 1 < page_len {
                    self.table_scroll = self.table_scroll.saturating_add(1);
                }
            }
            Up | Char('k') => {
                if self.table_scroll > 0 {
                    self.table_scroll = self.table_scroll.saturating_sub(1);
                }
            }
            Left | Char('h') => {
                // previous page
                if self.table_page > 0 {
                    self.table_page = self.table_page.saturating_sub(1);
                }
                self.table_scroll = 0;
                self.table_view_offset = 0;
            }
            Right | Char('l') => {
                // next page
                self.table_page = self.table_page.saturating_add(1);
                self.table_scroll = 0;
                self.table_view_offset = 0;
            }
            Char('q') => {
                // Back to cluster list (do not quit here). 'q' is the ONLY back key in ClusterTable.
                self.mode = Mode::ClusterList;
                self.selected_cluster_id = None;
            }
            Enter => {
                // open detail float for the selected row on current page
                // compute cached rows reference
                if let Some((cluster_id, _)) = self
                    .filtered_clusters()
                    .get(self.list_state.selected().unwrap_or(0))
                    .cloned()
                {
                    if let Some(cached_cid) = self.cached_cluster_id {
                        if cached_cid == cluster_id {
                            let total = self.cached_filtered_rows.len();
                            let total_pages = if total == 0 {
                                1
                            } else {
                                (total + TABLE_PAGE_SIZE - 1) / TABLE_PAGE_SIZE
                            };
                            let page = if self.table_page >= total_pages {
                                total_pages.saturating_sub(1)
                            } else {
                                self.table_page
                            };
                            let page_start = page.saturating_mul(TABLE_PAGE_SIZE);
                            let page_end = (page_start + TABLE_PAGE_SIZE).min(total);
                            let page_len = page_end.saturating_sub(page_start);
                            let idx_in_page = self.table_scroll.min(page_len.saturating_sub(1));
                            let abs_idx = page_start.saturating_add(idx_in_page);
                            if abs_idx < total {
                                if let Some(&ri) = self.cached_filtered_rows.get(abs_idx) {
                                    if let Ok(row) = self.table.get_row(ri) {
                                        // build detail lines from headers
                                        let mut lines = Vec::new();
                                        for (h, v) in self.table.headers.iter().zip(row.iter()) {
                                            lines.push(format!("{}: {}", h, v));
                                        }
                                        // push detail float
                                        let detail = DataDetail::new(lines);
                                        self.detail_float =
                                            Some(Float::new_absolute(Box::new(detail), 60, 12));
                                    }
                                }
                            }
                        }
                    }
                }
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

        // Draw detail float (top-most) if present
        if let Some(ref mut float) = self.detail_float {
            float.draw(f, f.area(), &self.theme);
        }

        // Draw loading float (highest priority) if present
        if let Some(ref mut float) = self.loading_float {
            float.draw(f, f.area(), &self.theme);
        }
    }

    fn mode_name(&self) -> &str {
        match self.mode {
            Mode::ClusterList => "ClusterList",
            Mode::ClusterTable => "ClusterTable",
        }
    }

    fn filtered_clusters(&mut self) -> Vec<(usize, usize)> {
        // Incremental cached cluster counting for ClusterList to avoid scanning entire dataset
        // Use last committed query/term (committed when user pressed Enter).
        // While the search bar is active, we must NOT perform live updates —
        // instead return the current cached cluster counts to freeze the UI.
        let query = self.committed_query.clone();
        let term = self.committed_term.to_lowercase();

        // If no filter, return exact counts cheaply
        if query.is_none() && term.is_empty() {
            return self
                .index
                .clusters
                .iter()
                .map(|c| (c.id, c.rows_idx.len()))
                .collect::<Vec<_>>();
        }

        // Build a simple key representing current filter to detect changes
        let filter_key = format!("{:?}|{}", query, term);

        // If cache key changed, reset cache
        if self.cluster_list_cache_key.as_deref() != Some(&filter_key) {
            self.cluster_list_cache_key = Some(filter_key.clone());
            self.cluster_list_cache.clear();
            for c in &self.index.clusters {
                self.cluster_list_cache.push((c.id, 0usize, 0usize));
            }
        }

        // If the search is currently active (user is typing), freeze results and
        // return the cached cluster counts rather than scanning.
        if self.filter.active() {
            if !self.cluster_list_cache.is_empty() {
                return self
                    .cluster_list_cache
                    .iter()
                    .filter(|(_, cnt, _)| *cnt > 0)
                    .map(|(id, cnt, _)| (*id, *cnt))
                    .collect();
            } else {
                return self
                    .index
                    .clusters
                    .iter()
                    .map(|c| (c.id, c.rows_idx.len()))
                    .collect::<Vec<_>>();
            }
        }

        // Helper to test a row against current committed query/term
        let headers = self.table.headers.clone();
        let matches_row = |row: &Vec<String>| -> bool {
            if let Some(ref q) = query {
                q.exprs.iter().all(|expr| {
                    if expr.key.is_empty() {
                        row.iter()
                            .any(|v| v.to_lowercase().contains(&expr.value.to_lowercase()))
                    } else if let Some(idx) = headers
                        .iter()
                        .position(|h| h.eq_ignore_ascii_case(&expr.key))
                    {
                        let val = &row[idx].to_lowercase();
                        let val_num = val.parse::<f64>().ok();
                        let target_num = expr.value.parse::<f64>().ok();
                        match expr.op {
                            crate::filter::SearchOp::Eq => val.contains(&expr.value.to_lowercase()),
                            crate::filter::SearchOp::EqExact => val == &expr.value.to_lowercase(),
                            crate::filter::SearchOp::NotEq => {
                                !val.contains(&expr.value.to_lowercase())
                            }
                            crate::filter::SearchOp::Gt => {
                                val_num.zip(target_num).map_or(false, |(a, b)| a > b)
                            }
                            crate::filter::SearchOp::Lt => {
                                val_num.zip(target_num).map_or(false, |(a, b)| a < b)
                            }
                            crate::filter::SearchOp::Ge => {
                                val_num.zip(target_num).map_or(false, |(a, b)| a >= b)
                            }
                            crate::filter::SearchOp::Le => {
                                val_num.zip(target_num).map_or(false, |(a, b)| a <= b)
                            }
                            crate::filter::SearchOp::Contains => {
                                val.contains(&expr.value.to_lowercase())
                            }
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
        };

        // Incremental scanning budget per call (to keep UI responsive)
        const BUDGET: usize = 800; // total rows to check across clusters per invocation
        let mut budget_left = BUDGET;

        // Iterate clusters and advance their scanning progress up to the budget
        for cache_entry in &mut self.cluster_list_cache {
            if budget_left == 0 {
                break;
            }
            // find the cluster struct
            if let Some(c) = self.index.clusters.iter().find(|cc| cc.id == cache_entry.0) {
                let total_rows = c.rows_idx.len();
                while cache_entry.2 < total_rows && budget_left > 0 {
                    let ri = c.rows_idx[cache_entry.2];
                    cache_entry.2 += 1;
                    budget_left = budget_left.saturating_sub(1);
                    if let Ok(row) = self.table.get_row(ri) {
                        if matches_row(&row) {
                            cache_entry.1 = cache_entry.1.saturating_add(1);
                        }
                    }
                }
            }
        }

        // Produce the output vector from current cache counts (may be partial during scanning)
        let mut out = Vec::new();
        for (id, count, _) in &self.cluster_list_cache {
            if *count > 0 {
                out.push((*id, *count));
            }
        }
        out
    }

    fn draw_cluster_list(&mut self, f: &mut Frame, area: Rect) {
        let items = self
            .filtered_clusters()
            .iter()
            .map(|(id, _n)| {
                let s = format!("Cluster {}", id);
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

    fn draw_preview(&mut self, f: &mut Frame, area: Rect) {
        // tampilkan beberapa baris dari cluster terpilih, TERFILTER oleh search
        let filtered = self.filtered_clusters();
        let sel_idx = self.list_state.selected().unwrap_or(0);
        let mut lines = Vec::new();

        if !filtered.is_empty() {
            // Clamp selected index into visible filtered list
            let idx = sel_idx.min(filtered.len().saturating_sub(1));
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

                // Use committed query/term when the search bar is active so preview doesn't live-update.
                let query = if self.filter.active() {
                    self.committed_query.clone()
                } else {
                    self.filter.parsed_query()
                };
                let term = if self.filter.active() {
                    self.committed_term.to_lowercase()
                } else {
                    self.filter.term().to_lowercase()
                };

                let mut filtered_rows: Vec<usize> = Vec::new();
                if term.is_empty() && query.is_none() {
                    filtered_rows = c.rows_idx.iter().cloned().collect::<Vec<_>>();
                } else {
                    for &ri in &c.rows_idx {
                        if let Ok(row) = self.table.get_row(ri) {
                            let ok;
                            if let Some(ref q) = query {
                                ok = q.exprs.iter().all(|expr| {
                                    if expr.key.is_empty() {
                                        row.iter().any(|v| {
                                            v.to_lowercase().contains(&expr.value.to_lowercase())
                                        })
                                    } else if let Some(idx) = self
                                        .table
                                        .headers
                                        .iter()
                                        .position(|h| h.eq_ignore_ascii_case(&expr.key))
                                    {
                                        let val = &row[idx].to_lowercase();
                                        let val_num = val.parse::<f64>().ok();
                                        let target_num = expr.value.parse::<f64>().ok();
                                        match expr.op {
                                            crate::filter::SearchOp::Eq => {
                                                val.contains(&expr.value.to_lowercase())
                                            }
                                            crate::filter::SearchOp::EqExact => {
                                                val == &expr.value.to_lowercase()
                                            }
                                            crate::filter::SearchOp::NotEq => {
                                                !val.contains(&expr.value.to_lowercase())
                                            }
                                            crate::filter::SearchOp::Gt => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a > b),
                                            crate::filter::SearchOp::Lt => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a < b),
                                            crate::filter::SearchOp::Ge => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a >= b),
                                            crate::filter::SearchOp::Le => val_num
                                                .zip(target_num)
                                                .map_or(false, |(a, b)| a <= b),
                                            crate::filter::SearchOp::Contains => {
                                                val.contains(&expr.value.to_lowercase())
                                            }
                                        }
                                    } else {
                                        false
                                    }
                                });
                            } else if term.is_empty() {
                                ok = true;
                            } else {
                                ok = row.iter().any(|v| v.to_lowercase().contains(&term));
                            }
                            if ok {
                                filtered_rows.push(ri);
                            }
                        }
                    }
                }

                // Tambahkan pesan jika hasil kosong
                if filtered_rows.is_empty() {
                    lines.push(
                        Line::from("No matching entries found.").style(
                            Style::default()
                                .fg(self.theme.unfocused_color())
                                .add_modifier(Modifier::ITALIC),
                        ),
                    );
                }

                // Render maksimal 10 baris hasil
                for &ri in filtered_rows.iter().take(10) {
                    let row = match self.table.get_row(ri) {
                        Ok(r) => r,
                        Err(_) => Vec::new(),
                    };
                    // bentuk ringkas: ip method url status size
                    let ip = self.pick("ip", &row).unwrap_or_default();
                    let method = self.pick("method", &row).unwrap_or_default();
                    let url = self.pick("url", &row).unwrap_or_default();
                    let status = self.pick("status", &row).unwrap_or_default();
                    let size = self.pick("size", &row).unwrap_or_default();
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

        // Determine which cluster to display in ClusterTable.
        // Prefer `selected_cluster_id` (set when user selected a cluster in ClusterList).
        // If it's missing or not present in current filtered list, fall back to the first cluster.
        let target = if let Some(sel_cid) = self.selected_cluster_id {
            filtered
                .iter()
                .find(|(id, _)| *id == sel_cid)
                .copied()
                .or_else(|| filtered.get(0).copied())
        } else {
            filtered.get(0).copied()
        };

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

        if let Some((cluster_id, _count)) = target {
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

            // Ensure we have a cached filtered list for this cluster+filter.
            if self.cached_cluster_id != Some(cluster_id) {
                // Recompute filtered rows (full scan) and cache the result.
                self.cached_filtered_rows.clear();
                // Use committed query/term while search is active so table does not live-update
                let query = if self.filter.active() {
                    self.committed_query.clone()
                } else {
                    self.filter.parsed_query()
                };
                let term = if self.filter.active() {
                    self.committed_term.to_lowercase()
                } else {
                    self.filter.term().to_lowercase()
                };
                for &ri in &c.rows_idx {
                    if let Ok(row) = self.table.get_row(ri) {
                        let ok = if let Some(ref q) = query {
                            q.exprs.iter().all(|expr| {
                                if expr.key.is_empty() {
                                    row.iter().any(|v| {
                                        v.to_lowercase().contains(&expr.value.to_lowercase())
                                    })
                                } else if let Some(idx) = self
                                    .table
                                    .headers
                                    .iter()
                                    .position(|h| h.eq_ignore_ascii_case(&expr.key))
                                {
                                    let val = &row[idx].to_lowercase();
                                    let val_num = val.parse::<f64>().ok();
                                    let target_num = expr.value.parse::<f64>().ok();
                                    match expr.op {
                                        crate::filter::SearchOp::Eq => {
                                            val.contains(&expr.value.to_lowercase())
                                        }
                                        crate::filter::SearchOp::EqExact => {
                                            val == &expr.value.to_lowercase()
                                        }
                                        crate::filter::SearchOp::NotEq => {
                                            !val.contains(&expr.value.to_lowercase())
                                        }
                                        crate::filter::SearchOp::Gt => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a > b)
                                        }
                                        crate::filter::SearchOp::Lt => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a < b)
                                        }
                                        crate::filter::SearchOp::Ge => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a >= b)
                                        }
                                        crate::filter::SearchOp::Le => {
                                            val_num.zip(target_num).map_or(false, |(a, b)| a <= b)
                                        }
                                        crate::filter::SearchOp::Contains => {
                                            val.contains(&expr.value.to_lowercase())
                                        }
                                    }
                                } else {
                                    false
                                }
                            })
                        } else if term.is_empty() {
                            true
                        } else {
                            row.iter().any(|v| v.to_lowercase().contains(&term))
                        };
                        if ok {
                            self.cached_filtered_rows.push(ri);
                        }
                    }
                }
                self.cached_cluster_id = Some(cluster_id);
                // Reset page/scroll when cache recomputed
                self.table_page = 0;
                self.table_scroll = 0;
                self.table_view_offset = 0;
            }

            let filtered_rows_idx = &self.cached_filtered_rows;

            // HEADER - clone headers to avoid holding an immutable borrow of self.table
            let headers_clone = self.table.headers.clone();
            let header = Row::new(headers_clone.iter().map(|h| {
                Cell::from(h.as_str()).style(
                    Style::default()
                        .fg(self.theme.table_header())
                        .bg(self.theme.border_color())
                        .add_modifier(Modifier::BOLD),
                )
            }));

            let mut rows = Vec::new();
            // TABLE ROWS — compute page start/limits and clamp
            let total = filtered_rows_idx.len();
            let total_pages = if total == 0 {
                1
            } else {
                (total + TABLE_PAGE_SIZE - 1) / TABLE_PAGE_SIZE
            };

            // Compute the absolute selected index from current page/scroll and ensure page/scroll
            // are adjusted so the selected row remains visible (scroll-to-selected).
            let mut abs_selected = self
                .table_page
                .saturating_mul(TABLE_PAGE_SIZE)
                .saturating_add(self.table_scroll);
            if total == 0 {
                abs_selected = 0;
                self.table_page = 0;
                self.table_scroll = 0;
            } else {
                if abs_selected >= total {
                    abs_selected = total.saturating_sub(1);
                }
                // compute page that contains the absolute selected and clamp into valid pages
                let new_page = abs_selected / TABLE_PAGE_SIZE;
                self.table_page = new_page.min(total_pages.saturating_sub(1));
            }

            // compute page bounds in outer scope so the rendering code can use them
            let page = self.table_page;
            let page_start = page.saturating_mul(TABLE_PAGE_SIZE);
            let page_end = (page_start + TABLE_PAGE_SIZE).min(total);
            let mut page_len = page_end.saturating_sub(page_start);
            if page_len == 0 {
                page_len = 1;
            }
            // adjust table_scroll to be relative to current page and within visible range
            if total == 0 {
                self.table_scroll = 0;
            } else {
                let rel = abs_selected.saturating_sub(page_start);
                self.table_scroll = rel.min(page_len.saturating_sub(1));
            }

            // Compute viewport within the page (visible rows) so selection is always visible
            // Estimate number of visible rows inside the table area: subtract 1 for table header and 2 for borders
            let table_area_height = layout[1].height as usize;
            let mut visible_rows = table_area_height.saturating_sub(3);
            if visible_rows == 0 {
                visible_rows = 1;
            }

            // Clamp view offset so it fits within page bounds
            if page_len <= visible_rows {
                self.table_view_offset = 0;
            } else if self.table_view_offset > page_len.saturating_sub(visible_rows) {
                self.table_view_offset = page_len.saturating_sub(visible_rows);
            }

            // Ensure selected row is within viewport; adjust view offset if needed
            if self.table_scroll < self.table_view_offset {
                self.table_view_offset = self.table_scroll;
            } else if self.table_scroll >= self.table_view_offset + visible_rows {
                self.table_view_offset = self
                    .table_scroll
                    .saturating_add(1)
                    .saturating_sub(visible_rows);
            }

            // Render only the slice visible inside the current page viewport
            let view_start = page_start.saturating_add(self.table_view_offset);
            let view_end = (view_start + visible_rows).min(page_end);

            for (i, ri) in filtered_rows_idx[view_start..view_end].iter().enumerate() {
                let r = match self.table.get_row(*ri) {
                    Ok(v) => v,
                    Err(_) => Vec::new(),
                };
                let bg = if i % 2 == 0 {
                    self.theme.table_row_even()
                } else {
                    self.theme.table_row_odd()
                };
                // If this row is focused within the page viewport, use selection style
                let is_focused = i == self.table_scroll.saturating_sub(self.table_view_offset);
                let cell_style_base = if is_focused {
                    Style::default()
                        .bg(self.theme.selection_bg())
                        .fg(self.theme.selection_fg())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(bg)
                };

                rows.push(Row::new(
                    r.into_iter().map(|v| Cell::from(v).style(cell_style_base)),
                ));
            }

            let table = ratatui::widgets::Table::new(rows, self.auto_widths(area.width))
                .header(header)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(format!(
                            " Cluster {} - {} rows  |  Page {}/{} ",
                            cluster_id,
                            filtered_rows_idx.len(),
                            page + 1,
                            total_pages
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
                    // Read rows on-demand; fall back to empty strings if read fails
                    let ra = match self.table.get_row(a) {
                        Ok(v) => v,
                        Err(_) => Vec::new(),
                    };
                    let rb = match self.table.get_row(b) {
                        Ok(v) => v,
                        Err(_) => Vec::new(),
                    };

                    let va = ra.get(idx).cloned().unwrap_or_default();
                    let vb = rb.get(idx).cloned().unwrap_or_default();

                    // Try parse to f64
                    let va_num = va.parse::<f64>();
                    let vb_num = vb.parse::<f64>();

                    let ord = if va_num.is_ok() && vb_num.is_ok() {
                        va_num
                            .unwrap()
                            .partial_cmp(&vb_num.unwrap())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        va.cmp(&vb)
                    };

                    if order == SortOrder::Ascend {
                        ord
                    } else {
                        ord.reverse()
                    }
                });
            }
            // Sorting changed — cached filtered results may be stale
            self.cached_cluster_id = None;
            self.cached_filtered_rows.clear();
            self.table_page = 0;
            self.table_scroll = 0;
            self.table_view_offset = 0;
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
                        ("Detail", ["Enter"]),
                        ("Scroll", ["j", "k", "↑", "↓"]),
                        ("Page", ["h", "l", "←", "→"]),
                        ("Sort", ["s"]),
                        ("Search", ["/"]),
                        ("Back", ["q"]),
                    ),
                ),
            }
        }
    }
}
