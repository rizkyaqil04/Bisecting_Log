// src/theme.rs
use clap::ValueEnum;
use ratatui::style::Color;

/// Tema utama aplikasi
#[derive(Clone, Debug, PartialEq, Default, ValueEnum, Copy)]
pub enum Theme {
    /// Warna default dengan kontras tinggi dan modern
    #[default]
    Default,
    /// Warna kompatibel dengan terminal klasik (lebih soft)
    Compatible,
}

impl Theme {
    // === Warna untuk bagian utama UI ===
    pub const fn header_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(0, 220, 255),
            Theme::Compatible => Color::LightBlue,
        }
    }

    pub const fn hits_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(255, 210, 0),
            Theme::Compatible => Color::Yellow,
        }
    }

    pub const fn visitors_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(120, 255, 120),
            Theme::Compatible => Color::Green,
        }
    }

    pub const fn method_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(255, 128, 0),
            Theme::Compatible => Color::LightYellow,
        }
    }

    pub const fn proto_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(150, 200, 255),
            Theme::Compatible => Color::Cyan,
        }
    }

    pub const fn data_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(220, 220, 220),
            Theme::Compatible => Color::Gray,
        }
    }

    pub const fn label_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(180, 180, 180),
            Theme::Compatible => Color::DarkGray,
        }
    }

    pub const fn value_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(255, 255, 255),
            Theme::Compatible => Color::White,
        }
    }

    // === Ikon atau simbol tampilan ===
    pub const fn tab_icon(&self) -> &'static str {
        match self {
            Theme::Default => " ",
            Theme::Compatible => ">> ",
        }
    }

    pub const fn success_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(0, 255, 100),
            Theme::Compatible => Color::Green,
        }
    }

    pub const fn fail_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(255, 80, 80),
            Theme::Compatible => Color::Red,
        }
    }

    pub const fn focused_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(0, 180, 255),
            Theme::Compatible => Color::LightBlue,
        }
    }

    pub const fn unfocused_color(&self) -> Color {
        match self {
            Theme::Default => Color::Gray,
            Theme::Compatible => Color::DarkGray,
        }
    }

    pub const fn background_color(&self) -> Color {
        match self {
            Theme::Default => Color::Rgb(20, 20, 20),
            Theme::Compatible => Color::Black,
        }
    }
}

impl Theme {
    /// Ganti ke tema berikutnya
    pub fn next(&mut self) {
        let position = *self as usize;
        let variants = Theme::value_variants();
        *self = variants[(position + 1) % variants.len()];
    }

    /// Ganti ke tema sebelumnya
    pub fn prev(&mut self) {
        let position = *self as usize;
        let variants = Theme::value_variants();
        *self = variants[(position + variants.len() - 1) % variants.len()];
    }
}
