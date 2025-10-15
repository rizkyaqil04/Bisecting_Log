mod terminal_check;
mod cli;
mod theme;
mod filter;
mod data;
mod state;
mod sort;
mod float;
mod hint;
mod gauge;
mod process;
mod quit;

use crate::{
    cli::Args,
    state::App,
    gauge::{GaugeState, render_gauge_ui},
    process::run_python,
};
use anyhow::Result;
use crossterm::{
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use terminal_check::{is_too_small, draw_too_small_warning};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let args = <Args as clap::Parser>::parse();
    let (input_log, csv_path) = match args.resolve_paths() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("\n{}", e);
            std::process::exit(1);
        }
    };

    // --- setup terminal
    let mut out = stdout();
    out.execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;
    term.clear()?;

    if let Some(input_path) = input_log {
        let gauge = Arc::new(Mutex::new(GaugeState::new()));
        let (tx, mut rx) = mpsc::unbounded_channel();

        // spawn python subprocess (jalankan parser & clustering)
        {
            let tx = tx.clone();
            let input_path_clone = input_path.clone();
            let csv_path_clone = csv_path.clone();

            tokio::spawn(async move {
                run_python(&input_path_clone, &csv_path_clone, args.n_clusters, move |msg| {
                    let _ = tx.send(msg);
                })
                .await;
            });
        }

        // --- gauge python process loop
        loop {
            let size = term.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);

            if is_too_small(area) {
                term.draw(|f| draw_too_small_warning(f, area))?;
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            term.draw(|f| render_gauge_ui(f, &gauge.lock().unwrap()))?;

            if let Ok(msg) = rx.try_recv() {
                gauge.lock().unwrap().update(&msg);
            }

            if gauge.lock().unwrap().done {
                break;
            }

            if crossterm::event::poll(std::time::Duration::from_millis(50))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.code == crossterm::event::KeyCode::Char('q') {
                        disable_raw_mode()?;
                        term.show_cursor()?;
                        return Ok(());
                    }
                }
            }
        }
    }

    // --- main app loop
    let mut app = App::new(args)?;
    let res = app.run(&mut term);

    // restore terminal
    disable_raw_mode()?;
    let backend = term.backend_mut();
    backend.execute(LeaveAlternateScreen)?;
    term.show_cursor()?;

    res
}
