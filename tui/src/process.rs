use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use std::process::Stdio;
use std::path::PathBuf;

pub async fn run_python(
    input_path: &PathBuf,
    output_path: &PathBuf,
    n_clusters: usize,
    callback: impl Fn(String) + Send + Sync + 'static,
) {
    let mut child = Command::new("python3")
        .arg("./app/run.py")
        .arg("--input")
        .arg(input_path)
        .arg("--output")
        .arg(output_path)
        .arg("--n")
        .arg(n_clusters.to_string())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Gagal menjalankan proses python");

    let stdout = child.stdout.take().expect("Tidak ada stdout");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        callback(line);
    }
}
