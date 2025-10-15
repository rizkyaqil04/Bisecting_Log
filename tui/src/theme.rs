use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Default)]
pub enum Theme {
    #[default]
    Default,
}

impl Theme {
    pub const fn title_color(&self) -> Color {
        Color::Yellow
    }
    pub const fn cluster_color(&self) -> Color {
        Color::Cyan
    }
    pub const fn preview_color(&self) -> Color {
        Color::Gray
    }
    pub const fn table_header(&self) -> Color {
        Color::Black
    }
    // pub const fn table_text(&self) -> Color { Color::White }
    pub const fn selection_bg(&self) -> Color {
        Color::Blue
    }
    pub const fn selection_fg(&self) -> Color {
        Color::Black
    }
    pub const fn focused_color(&self) -> Color {
        Color::LightBlue
    }
    pub const fn unfocused_color(&self) -> Color {
        Color::Gray
    }
    pub const fn table_row_even(&self) -> Color {
        Color::Rgb(40, 44, 52)
    }
    pub const fn table_row_odd(&self) -> Color {
        Color::Rgb(30, 34, 40)
    }
    pub const fn info_color(&self) -> Color {
        Color::LightRed
    }
    pub const fn border_color(&self) -> Color {
        Color::LightBlue
    }
}
