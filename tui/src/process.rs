use std::{env, path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

use tokio::sync::oneshot;

/// Spawn the python process and return a shutdown sender and a JoinHandle for the background task.
/// Sending on the returned `oneshot::Sender` will attempt to kill the child process and stop reading.
pub fn spawn_python_with_shutdown(
    input_path: &PathBuf,
    output_path: &PathBuf,
    n_clusters: usize,
    callback: impl Fn(String) + Send + Sync + 'static,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let input_path = input_path.clone();
    let output_path = output_path.clone();
    let cb = std::sync::Arc::new(callback);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        // Deteksi environment conda aktif
        let python_exec = match env::var("CONDA_PREFIX") {
            Ok(prefix) => {
                if cfg!(target_os = "windows") {
                    PathBuf::from(prefix).join("python.exe")
                } else {
                    PathBuf::from(prefix).join("bin").join("python3")
                }
            }
            Err(_) => {
                if cfg!(target_os = "windows") {
                    PathBuf::from("python")
                } else {
                    PathBuf::from("python3")
                }
            }
        };

        let mut child = match Command::new(&python_exec)
            .arg("./app/run.py")
            .arg("--input")
            .arg(&input_path)
            .arg("--output")
            .arg(&output_path)
            .arg("--n")
            .arg(n_clusters.to_string())
            .stdout(Stdio::piped())
            // hide noisy stderr from underlying libraries (transformers/torch)
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = cb(format!("ERROR: Failed to spawn python: {}", e));
                return;
            }
        };

        let stdout = child.stdout.take();
        if stdout.is_none() {
            let _ = cb("ERROR: No stdout from python process".to_string());
            return;
        }
        let reader = BufReader::new(stdout.unwrap());
        let mut lines = reader.lines();

        // shutdown receiver used in select
        let mut shutdown_rx = shutdown_rx;

        loop {
            tokio::select! {
                line = lines.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                // filter only well-formed messages produced by ProgressManager
                                let s = l.trim();
                                let forward = s.starts_with("PROGRESS: ")
                                    || s.starts_with("STATUS: ")
                                    || s == "DONE"
                                    || s.starts_with("ERROR: ")
                                    || s.contains("Traceback")
                                    || s.contains("Exception");

                                if forward {
                                    (cb)(s.to_string());
                                } else {
                                    // ignore noisy/unrelated lines (embedding logs, transformers info, etc.)
                                }
                            }
                        Ok(None) => break, // EOF
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    // request to shutdown
                    let _ = child.kill().await;
                    break;
                }
            }
        }

        // ensure child terminated
        let _ = child.kill().await;
    });

    (shutdown_tx, handle)
}
