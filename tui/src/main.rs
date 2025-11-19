mod terminal_check;
mod cli;
mod theme;
mod filter;
mod data;
mod detail;
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

        // spawn python subprocess (jalankan parser & clustering) with shutdown control
        let mut shutdown_opt: Option<tokio::sync::oneshot::Sender<()>>;
        let mut python_handle_opt: Option<tokio::task::JoinHandle<()>>;
        {
            let tx = tx.clone();
            let input_path_clone = input_path.clone();
            let csv_path_clone = csv_path.clone();

            let (shutdown_tx, handle) = process::spawn_python_with_shutdown(
                &input_path_clone,
                &csv_path_clone,
                args.n_clusters,
                move |msg| {
                    let _ = tx.send(msg);
                },
            );

            shutdown_opt = Some(shutdown_tx);
            python_handle_opt = Some(handle);
        }

        use std::time::{Instant, Duration};
        use std::fs;
        let mut last_log_update = Instant::now() - Duration::from_secs(1);

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

            // drain any messages and update gauge state
            if let Ok(msg) = rx.try_recv() {
                gauge.lock().unwrap().update(&msg);
            }

            // periodically read the status log and display tail in message box
            if last_log_update.elapsed() >= Duration::from_millis(500) {
                last_log_update = Instant::now();

                // derive base output name from csv_path (strip .gz, .csv, .txt)
                let mut base = csv_path.clone();
                loop {
                    if let Some(ext) = base.extension() {
                        let ext_s = ext.to_string_lossy().to_lowercase();
                        if ext_s == "gz" || ext_s == "csv" || ext_s == "txt" {
                            base.set_extension("");
                            continue;
                        }
                    }
                    break;
                }

                let file_stem = base.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "output".into());
                let log_path = base.parent().map(|p| p.join(format!("{}_status.log", file_stem))).unwrap_or_else(|| std::path::PathBuf::from(format!("{}_status.log", file_stem)));

                let mut gs = gauge.lock().unwrap();
                if let Ok(s) = fs::read_to_string(&log_path) {
                    let lines: Vec<&str> = s.lines().collect();
                    let start = if lines.len() > 12 { lines.len() - 12 } else { 0 };
                    let tail = lines[start..].join("\n");
                    gs.message = if tail.trim().is_empty() { None } else { Some(tail) };
                    gs.message_error = gs.message.as_ref().map_or(false, |m| m.contains("Traceback") || m.contains("ERROR"));
                } else {
                    // no log yet -> hide message
                    gs.message = None;
                    gs.message_error = false;
                }
            }

            if gauge.lock().unwrap().done {
                break;
            }

            if crossterm::event::poll(std::time::Duration::from_millis(50))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.code == crossterm::event::KeyCode::Char('q') {
                        // request python shutdown and wait for it to finish to avoid broken pipe
                        if let Some(sh) = shutdown_opt.take() {
                            let _ = sh.send(());
                        }
                        if let Some(h) = python_handle_opt.take() {
                            let _ = h.await;
                        }

                        disable_raw_mode()?;
                        let backend = term.backend_mut();
                        backend.execute(LeaveAlternateScreen)?;
                        term.show_cursor()?;
                        return Ok(());
                    }
                }
            }
        }

        // if loop exited naturally (done), ensure python background task is stopped
        if let Some(sh) = shutdown_opt.take() {
            let _ = sh.send(());
        }
        if let Some(h) = python_handle_opt.take() {
            let _ = h.await;
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
