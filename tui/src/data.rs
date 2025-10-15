use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use std::{collections::HashMap, fs::File, io::Read, path::Path};

#[derive(Clone, Debug)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h.eq_ignore_ascii_case(name))
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
    let mut data = String::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("gz") {
            let f = File::open(path)?;
            let mut gz = GzDecoder::new(f);
            gz.read_to_string(&mut data)?;
        } else {
            let mut f = File::open(path)?;
            f.read_to_string(&mut data)?;
        }
    } else {
        let mut f = File::open(path)?;
        f.read_to_string(&mut data)?;
    }

    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(data.as_bytes());
    let headers = rdr.headers()?.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        rows.push(rec.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    }
    Ok(Table { headers, rows })
}

pub fn build_cluster_index(table: &Table) -> Result<ClusterIndex> {
    let Some(cidx) = table.column_index("cluster") else {
        return Err(anyhow!("Kolom 'cluster' tidak ditemukan. Tambahkan kolom ini di hasil klustering."));
    };
    let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, row) in table.rows.iter().enumerate() {
        let id: usize = row.get(cidx)
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| anyhow!("Nilai cluster tidak valid pada baris {}", i))?;
        map.entry(id).or_default().push(i);
    }
    let mut clusters: Vec<Cluster> = map.into_iter()
        .map(|(id, rows_idx)| Cluster { id, rows_idx })
        .collect();
    clusters.sort_by_key(|c| c.id);
    Ok(ClusterIndex { clusters })
}
