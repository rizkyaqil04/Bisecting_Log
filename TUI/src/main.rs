mod state;

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyEventKind, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{stdout, Result},
    time::{Duration, Instant},
};
use state::AppState;

#[derive(Parser, Debug, Clone)]
struct Args {
    /// Aktifkan dukungan mouse
    #[arg(long)]
    mouse: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut state = AppState::new();

    // masuk ke alternate screen dan aktifkan raw mode
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    if args.mouse {
        execute!(out, EnableMouseCapture)?;
    }
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    terminal.clear()?;

    run(&mut terminal, &mut state, args.mouse)?;

    // kembalikan terminal ke mode normal
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    if args.mouse {
        execute!(stdout(), DisableMouseCapture)?;
    }
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
    _mouse_enabled: bool,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(200);

    while app.running {
        terminal.draw(|f| {
            let area = f.area();           // ambil ukuran terminal
            app.draw(f, area);             // panggil draw dengan Rect
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                    // handle_key tidak lagi mengembalikan bool
                    app.handle_key(&key);
                }
            }
            // jika butuh mouse di masa depan, tambahkan match Event::Mouse di sini
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

