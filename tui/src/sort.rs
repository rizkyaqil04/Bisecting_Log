use crate::{float::FloatContent, hint::Shortcut, shortcuts, theme::Theme};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascend,
    Descend,
}

#[derive(Debug, Clone)]
pub struct SortMenu {
    pub columns: Vec<String>,
    pub selected_col: usize,
    pub selected_order: SortOrder,
    pub cursor_panel: usize,
    pub sortby_cursor: usize,
    pub sortby_scroll: usize,
    last_visible_height: usize,
    pub order_cursor: usize,
    pub order_scroll: usize,
    order_last_visible_height: usize,
    pub finished: bool,
    pub cancelled: bool,
}

impl SortMenu {
    pub fn new(columns: Vec<String>, default_col: usize, default_order: SortOrder) -> Self {
        Self {
            columns,
            selected_col: default_col,
            selected_order: default_order,
            cursor_panel: 0,
            sortby_cursor: default_col,
            sortby_scroll: 0,
            last_visible_height: 0,
            order_cursor: if matches!(default_order, SortOrder::Descend) {
                1
            } else {
                0
            },
            order_scroll: 0,
            order_last_visible_height: 0,
            finished: false,
            cancelled: false,
        }
    }

    fn ensure_cursor_in_view(&mut self) {
        if self.last_visible_height == 0 {
            return;
        }
        let start = self.sortby_scroll;
        let end = self
            .sortby_scroll
            .saturating_add(self.last_visible_height.saturating_sub(1));
        if self.sortby_cursor < start {
            self.sortby_scroll = self.sortby_cursor;
        } else if self.sortby_cursor > end {
            self.sortby_scroll = self
                .sortby_cursor
                .saturating_sub(self.last_visible_height - 1);
        }
    }

    fn ensure_order_cursor_in_view(&mut self, _order_len: usize) {
        if self.order_last_visible_height == 0 {
            return;
        }
        let start = self.order_scroll;
        let end = self
            .order_scroll
            .saturating_add(self.order_last_visible_height.saturating_sub(1));
        if self.order_cursor < start {
            self.order_scroll = self.order_cursor;
        } else if self.order_cursor > end {
            self.order_scroll = self
                .order_cursor
                .saturating_sub(self.order_last_visible_height - 1);
        }
    }

    fn move_down(&mut self) {
        match self.cursor_panel {
            0 => {
                if self.sortby_cursor + 1 < self.columns.len() {
                    self.sortby_cursor += 1;
                    self.ensure_cursor_in_view();
                }
            }
            1 => {
                let order_len = 2;
                if self.order_cursor + 1 < order_len {
                    self.order_cursor += 1;
                    self.ensure_order_cursor_in_view(order_len);
                }
            }
            _ => {}
        }
    }

    fn move_up(&mut self) {
        match self.cursor_panel {
            0 => {
                if self.sortby_cursor > 0 {
                    self.sortby_cursor -= 1;
                    self.ensure_cursor_in_view();
                }
            }
            1 => {
                if self.order_cursor > 0 {
                    self.order_cursor -= 1;
                    self.ensure_order_cursor_in_view(2);
                }
            }
            _ => {}
        }
    }

    fn switch_focus(&mut self) {
        self.cursor_panel = (self.cursor_panel + 1) % 2;
    }

    fn choose_current(&mut self) {
        match self.cursor_panel {
            0 => self.selected_col = self.sortby_cursor,
            1 => {
                self.selected_order = if self.order_cursor == 0 {
                    SortOrder::Ascend
                } else {
                    SortOrder::Descend
                }
            }
            _ => {}
        }
    }
}

impl FloatContent for SortMenu {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        frame.render_widget(Clear, area);
        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Sorting Options ")
            .title_alignment(Alignment::Center);
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
            .split(inner);

        // ==== SORT BY ====
        let visible_height = layout[0].height.saturating_sub(2) as usize;
        self.last_visible_height = visible_height.max(1);
        self.ensure_cursor_in_view();

        let visible_items: Vec<_> = self
            .columns
            .iter()
            .enumerate()
            .skip(self.sortby_scroll)
            .take(self.last_visible_height)
            .map(|(i, col)| {
                let mark = if i == self.selected_col { "[x]" } else { "[ ]" };
                let mut item = ListItem::new(format!("{mark} {col}"));
                if self.cursor_panel == 0 && i == self.sortby_cursor {
                    item = item.style(
                        Style::default()
                            .fg(theme.selection_fg())
                            .bg(theme.selection_bg())
                            .add_modifier(Modifier::BOLD),
                    );
                }
                item
            })
            .collect();

        let sort_block = Block::default()
            .borders(Borders::ALL)
            .title(" Sort By ")
            .border_type(BorderType::Rounded)
            .border_style(if self.cursor_panel == 0 {
                Style::default().fg(theme.focused_color())
            } else {
                Style::default().fg(theme.unfocused_color())
            });

        frame.render_widget(List::new(visible_items).block(sort_block), layout[0]);

        // ==== ORDER ====
        let orders = ["Ascend", "Descend"]; // Ganti ke Vec<String> jika ingin dinamis
        let order_len = orders.len();
        let order_visible_height = layout[1].height.saturating_sub(2) as usize;
        self.order_last_visible_height = order_visible_height.max(1);
        self.ensure_order_cursor_in_view(order_len);

        let order_items: Vec<_> = orders
            .iter()
            .enumerate()
            .skip(self.order_scroll)
            .take(self.order_last_visible_height)
            .map(|(i, name)| {
                let mark = match (i, self.selected_order) {
                    (0, SortOrder::Ascend) => "[x]",
                    (1, SortOrder::Descend) => "[x]",
                    _ => "[ ]",
                };
                let mut item = ListItem::new(format!("{mark} {name}"));
                if self.cursor_panel == 1 && i == self.order_cursor {
                    item = item.style(
                        Style::default()
                            .fg(theme.selection_fg())
                            .bg(theme.selection_bg())
                            .add_modifier(Modifier::BOLD),
                    );
                }
                item
            })
            .collect();

        let order_block = Block::default()
            .borders(Borders::ALL)
            .title(" Order ")
            .border_type(BorderType::Rounded)
            .border_style(if self.cursor_panel == 1 {
                Style::default().fg(theme.focused_color())
            } else {
                Style::default().fg(theme.unfocused_color())
            });
        frame.render_widget(List::new(order_items).block(order_block), layout[1]);
    }

    fn handle_key_event(&mut self, key: &KeyEvent) -> bool {
        use KeyCode::*;
        match key.code {
            Char('q') | Esc => {
                self.finished = true;
                self.cancelled = true;
            }
            Enter => {
                self.finished = true;
            }
            Tab => self.switch_focus(),
            Char('j') | Down => self.move_down(),
            Char('k') | Up => self.move_up(),
            Char(' ') => self.choose_current(),
            _ => {}
        }
        self.finished
    }

    fn is_finished(&self) -> bool {
        self.finished
    }

    fn get_shortcut_list(&self) -> (&str, Box<[Shortcut]>) {
        (
            "Sort Menu",
            shortcuts!(
                ("Move selection", ["j", "k", "↑", "↓"]),
                ("Switch panel", ["Tab"]),
                ("Select option", ["Space"]),
                ("Confirm", ["Enter"]),
                ("Cancel", ["q", "Esc"])
            ),
        )
    }
}
