// src/io.rs

use std::{
    collections::HashMap,
    fs::{self, File},
    io,
    path::PathBuf,
};
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use serde::Deserialize;

/// Representasi satu baris hasil klustering
#[derive(Debug, Deserialize, Clone)]
pub struct ClusteredLog {
    pub ip: String,
    pub time: String,
    pub method: String,
    pub url: String,
    pub protocol: String,
    pub status: u16,
    pub size: u64,
    pub referrer: String,
    pub user_agent: String,
    pub cluster: u8,
}

/// Membaca file hasil klustering terbaru dari folder `path`.
/// Mendukung format `.csv` dan `.csv.gz`.
pub fn read_latest_csv(path: &str) -> io::Result<Vec<ClusteredLog>> {
    let mut files: Vec<_> = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_string();
            name.ends_with(".csv") || name.ends_with(".csv.gz")
        })
        .collect();

    if files.is_empty() {
        return Ok(Vec::new());
    }

    // urutkan berdasarkan waktu modifikasi terbaru
    files.sort_by_key(|f| f.metadata().unwrap().modified().unwrap());
    let latest_file = files.last().unwrap();
    let file_path = latest_file.path();

    let data = read_csv_file(&file_path)?;
    Ok(data)
}

/// Membaca file CSV (baik .csv maupun .csv.gz)
fn read_csv_file(path: &PathBuf) -> io::Result<Vec<ClusteredLog>> {
    let is_gz = path.extension().map(|e| e == "gz").unwrap_or(false);
    let file = File::open(path)?;

    let reader: Box<dyn io::Read> = if is_gz {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let mut data = Vec::new();
    for record in rdr.deserialize() {
        let entry: ClusteredLog =
            record.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        data.push(entry);
    }

    Ok(data)
}

/// Mengelompokkan hasil log berdasarkan nomor cluster
pub fn group_by_cluster(data: &[ClusteredLog]) -> HashMap<u8, Vec<ClusteredLog>> {
    let mut grouped: HashMap<u8, Vec<ClusteredLog>> = HashMap::new();
    for log in data {
        grouped.entry(log.cluster).or_default().push(log.clone());
    }
    grouped
}

/// Membersihkan hasil lama dan menyisakan `keep` file terbaru.
/// Misalnya dipanggil saat aplikasi mulai.
pub fn rotate_results(path: &str, keep: usize) -> io::Result<()> {
    let mut files: Vec<_> = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_string();
            name.ends_with(".csv") || name.ends_with(".csv.gz")
        })
        .collect();

    files.sort_by_key(|f| f.metadata().unwrap().modified().unwrap());
    while files.len() > keep {
        let old = files.remove(0);
        println!(
            "[rotate_results] Removing old result file: {:?}",
            old.path().display()
        );
        fs::remove_file(old.path())?;
    }

    Ok(())
}