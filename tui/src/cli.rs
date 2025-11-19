use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser};
use ratatui::crossterm::style::Stylize;
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Path to the log input file (.log or .txt)
    #[arg(short = 'i', long)]
    pub input: Option<PathBuf>,

    /// Path for saving the clustering output (optional)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Path to the clustering result file (.csv or .csv.gz)
    #[arg(short = 'r', long)]
    pub read: Option<PathBuf>,

    /// Number of clusters
    #[arg(short = 'n', long, default_value_t = 8)]
    pub n_clusters: usize,
}

impl Args {
    pub fn resolve_paths(&self) -> Result<(Option<PathBuf>, PathBuf)> {
        // Cegah penggunaan --read bersamaan dengan --n-clusters
        if self.read.is_some() && std::env::args().any(|a| a == "-n" || a == "--n-clusters") {
            bail!(
                "{} {}",
                "[PROHIBITED]".red().bold(),
                "--n-clusters cannot be used together with --read. The cluster count is already fixed in the CSV file."
            );
        }

        match (&self.input, &self.read) {
            // Hanya input log/txt
            (Some(input_path), None) => {
                let ext = input_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !["log", "txt"].contains(&ext.as_str()) {
                    bail!(
                        "{} The --input argument must be a .log or .txt file (not .{})",
                        "[ERROR]".red().bold(),
                        ext
                    );
                }

                let file_name = input_path
                    .file_stem()
                    .context("Failed to read file name from input path")?
                    .to_string_lossy();

                let output_path = if let Some(out) = &self.output {
                    out.clone()
                } else {
                    PathBuf::from(format!("./outputs/{file_name}.csv.gz"))
                };

                Ok((Some(input_path.clone()), output_path))
            }

            // Hanya read csv/gz
            (None, Some(read_path)) => {
                let ext = read_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !["csv", "gz"].contains(&ext.as_str()) {
                    bail!(
                        "{} --read must be a .csv or .csv.gz file (found .{})",
                        "[ERROR]".red().bold(),
                        ext
                    );
                }

                Ok((None, read_path.clone()))
            }

            // Kedua argumen tidak boleh bersamaan
            (Some(_), Some(_)) => {
                bail!(
                    "{} Cannot use both --input and --read. Choose one.",
                    "[PROHIBITED]".red().bold()
                );
            }

            // Tidak ada argumen apapun
            (None, None) => {
                // Tampilkan help kalau tidak ada argumen sama sekali
                println!("{} No argument given", "[ERROR]".red().bold());
                let mut cmd = Args::command();
                cmd.print_help().expect("Failed to print help message");
                println!(); // newline biar rapi
                std::process::exit(1);
            }
        }
    }
}
