use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use std::{fs::File, io::Read, path::{Path, PathBuf}};
use std::io::BufReader;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Table {
    pub headers: Vec<String>,
    // If `rows` is `Some`, the table is fully loaded into memory.
    // If `None`, the table is backed by a file at `path` and rows are loaded on demand.
    pub rows: Option<Vec<Vec<String>>>,
    pub path: Option<PathBuf>,
    pub is_gz: bool,
    // total number of data rows (excluding header). Filled during initial scan.
    pub total_rows: usize,
    // simple page cache: cached_page_start -> Vec<Vec<String>>
    pub page_cache_start: Option<usize>,
    pub page_cache: Option<Vec<Vec<String>>>,
}

impl Table {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h.eq_ignore_ascii_case(name))
    }

    /// Get a row by index. Loads pages from file when table is not fully loaded.
    pub fn get_row(&mut self, idx: usize) -> Result<Vec<String>> {
        if let Some(rows) = &self.rows {
            return Ok(rows.get(idx).cloned().unwrap_or_default());
        }

        // lazy load pages of 128 rows
        const PAGE_SIZE: usize = 128;
        let page_start = (idx / PAGE_SIZE) * PAGE_SIZE;

        if let Some(start) = self.page_cache_start {
            if start == page_start {
                if let Some(cache) = &self.page_cache {
                    return Ok(cache.get(idx - start).cloned().unwrap_or_default());
                }
            }
        }

        // need to load page from disk
        let path = match &self.path {
            Some(p) => p.clone(),
            None => return Ok(Vec::new()),
        };

        if self.is_gz {
            // fallback: for gz files, read all into memory (less common) then serve
            let full = read_csv_or_gz(&path)?;
            if let Some(rows) = full.rows {
                self.rows = Some(rows);
                self.total_rows = self.rows.as_ref().map(|r| r.len()).unwrap_or(0);
                return Ok(self.rows.as_ref().unwrap().get(idx).cloned().unwrap_or_default());
            } else {
                return Ok(Vec::new());
            }
        }

        // open file and iterate to page_start, then collect PAGE_SIZE records
        let f = File::open(&path)?;
        let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(f));
        let mut out = Vec::new();
        for (i, rec) in rdr.records().enumerate() {
            let rec = rec?;
            if i < page_start { continue; }
            if i >= page_start + PAGE_SIZE { break; }
            out.push(rec.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        }

        self.page_cache_start = Some(page_start);
        self.page_cache = Some(out.clone());
        Ok(out.get(idx - page_start).cloned().unwrap_or_default())
    }
}

#[derive(Clone, Debug)]
pub struct ClusterIndex {
    pub clusters: Vec<Cluster>,
}

#[derive(Clone, Debug)]
pub struct Cluster {
    pub id: usize,
    pub rows_idx: Vec<usize>, // index ke Table.rows
}

pub fn read_csv_or_gz(path: &Path) -> Result<Table> {
    let mut is_gz = false;
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("gz") {
            is_gz = true;
        }
    }

    // If gz -> fallback to full load (seeking in gz is not implemented here)
    if is_gz {
        let mut data = String::new();
        let f = File::open(path)?;
        let mut gz = GzDecoder::new(f);
        gz.read_to_string(&mut data)?;
        let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(data.as_bytes());
        let headers = rdr.headers()?.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut rows = Vec::new();
        for rec in rdr.records() {
            let rec = rec?;
            rows.push(rec.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        }
        return Ok(Table { headers, rows: Some(rows), path: Some(path.to_path_buf()), is_gz: true, total_rows: 0, page_cache_start: None, page_cache: None });
    }

    // For non-gz CSV, do an initial scan to capture headers and total rows, but don't keep rows in memory.
    let f = File::open(path)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(f));
    let headers = rdr.headers()?.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut total = 0usize;
    for rec in rdr.records() {
        let _ = rec?;
        total += 1;
    }

    Ok(Table { headers, rows: None, path: Some(path.to_path_buf()), is_gz: false, total_rows: total, page_cache_start: None, page_cache: None })
}

pub fn build_cluster_index(table: &Table) -> Result<ClusterIndex> {
    let Some(cidx) = table.column_index("cluster") else {
        return Err(anyhow!("Kolom 'cluster' tidak ditemukan. Tambahkan kolom ini di hasil klustering."));
    };

    let mut map: HashMap<usize, Vec<usize>> = HashMap::new();

    if let Some(rows) = &table.rows {
        for (i, row) in rows.iter().enumerate() {
            let id: usize = row.get(cidx)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or_else(|| anyhow!("Nilai cluster tidak valid pada baris {}", i))?;
            map.entry(id).or_default().push(i);
        }
    } else if let Some(path) = &table.path {
        // Scan file, parse cluster column for each record but don't keep rows
        if table.is_gz {
            // fallback: read all into memory then scan
            let full = read_csv_or_gz(path)?;
            if let Some(rows) = full.rows {
                for (i, row) in rows.iter().enumerate() {
                    let id: usize = row.get(cidx)
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or_else(|| anyhow!("Nilai cluster tidak valid pada baris {}", i))?;
                    map.entry(id).or_default().push(i);
                }
            }
        } else {
            let f = File::open(path)?;
            let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(f));
            for (i, rec) in rdr.records().enumerate() {
                let rec = rec?;
                let val = rec.get(cidx).ok_or_else(|| anyhow!("Missing cluster value at row {}", i))?;
                let id: usize = val.parse::<usize>().map_err(|_| anyhow!("Nilai cluster tidak valid pada baris {}", i))?;
                map.entry(id).or_default().push(i);
            }
        }
    }

    let mut clusters: Vec<Cluster> = map.into_iter()
        .map(|(id, rows_idx)| Cluster { id, rows_idx })
        .collect();
    clusters.sort_by_key(|c| c.id);
    Ok(ClusterIndex { clusters })
}
