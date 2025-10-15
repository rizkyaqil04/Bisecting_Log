use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use std::{env, path::PathBuf, process::Stdio};

pub async fn run_python(
    input_path: &PathBuf,
    output_path: &PathBuf,
    n_clusters: usize,
    callback: impl Fn(String) + Send + Sync + 'static,
) {
    // Deteksi environment conda aktif
    let python_exec = match env::var("CONDA_PREFIX") {
        Ok(prefix) => {
            // Buat path python dinamis sesuai OS
            if cfg!(target_os = "windows") {
                PathBuf::from(prefix).join("python.exe")
            } else {
                PathBuf::from(prefix).join("bin").join("python3")
            }
        }
        Err(_) => {
            // Kalau gak ada conda aktif, fallback ke default "python"/"python3"
            if cfg!(target_os = "windows") {
                PathBuf::from("python")
            } else {
                PathBuf::from("python3")
            }
        }
    };

    let mut child = Command::new(&python_exec)
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
