# Bisecting_Log

Bisecting_Log is a web log analysis tool that combines Bisecting KMeans clustering, advanced feature extraction, and a terminal-based user interface (TUI) for interactive log data exploration.

<!-- ![Screenshot](https://github.com/user-attachments/assets/813626c5-3bd3-4167-ab90-77ac4da09d01) -->

## Features

- **Automatic Log Clustering:** Groups web access logs using Bisecting KMeans, leveraging features such as URL embeddings (using BERT), HTTP method, status code, response size, and user-agent.
- **Feature Extraction & Tokenization:**
  - URL tokenization and number masking for robust pattern recognition.
  - User-agent tokenization and TF-IDF vectorization.
  - Status code categorization and one-hot encoding for HTTP methods and status.
  - Response size normalization.
- **Bot Detection:** Automatically skips entries from known bots/crawlers.
- **Progress Bar & Status:** Real-time progress and status updates during clustering.
- **Interactive TUI:** Explore clustering results, search, filter, and sort directly in the terminal.
- **Large File Support:** Accepts `.log` and `.txt` log files as input, outputs clustering results in `.csv` and `.csv.gz`.

## Project Structure

```
Bisecting_Log/
├── app/         # Python code for parsing, feature extraction, clustering
├── tui/         # Rust code for TUI (terminal user interface)
├── inputs/      # Raw log files
├── outputs/     # Clustering results (.csv, .txt, .csv.gz)
├── README.md
├── LICENSE
```

## Usage

### 1. Set Up Environment

**Python:**

- Python 3.8+
- Install dependencies:
  ```sh
  pip install -r app/requirements.txt
  ```

### 2. Run Clustering and TUI

You can use the `bkm-log` command to run clustering and/or open the TUI. The command usage is the same for both Linux and Windows, but the executable name differs:

#### On Linux

Use the compiled binary directly:

```sh
./bkm-log -i inputs/sample.log -n 10
```

#### On Windows

Use the `.exe` extension for the executable:

```bat
.\bkm-log.exe -i .\inputs\sample.log -n 10
```

#### Command Options

```
Usage: bkm-log [OPTIONS]

Options:
  -i, --input <INPUT>            Path to the log input file (.log or .txt)
  -o, --output <OUTPUT>          Path for saving the clustering output (optional)
  -r, --read <READ>              Path to the clustering result file (.csv or .csv.gz)
  -n, --n-clusters <N_CLUSTERS>  Number of clusters [default: 8]
  -h, --help                     Print help
```

#### Example Commands

- **Cluster a log file and open the TUI:**

  ```sh
  ./bkm-log -i inputs/sample.log -n 10
  ```

  This will process `inputs/sample.log` with 10 clusters, save the result, and launch the TUI automatically.

- **Open the TUI with an existing clustering result:**

  ```sh
  ./bkm-log -r outputs/sample.csv
  ```

  This will open the TUI and load the clustering result from `outputs/sample.csv`.

- **Specify a custom output file for clustering:**

  ```sh
  ./bkm-log -i inputs/sample.log -o outputs/custom.csv -n 8
  ```

  This will cluster the log and save the result to `outputs/custom.csv`.

### 3. TUI Navigation

- Navigation: `←/→` switch tab, `↑/↓` select item
- Details: `Enter`
- Search: `/`
- Quit: `q`
- See the full shortcut list at the bottom of the app.

## Output

- `outputs/sample.csv` — Clustering result (table format)
- `outputs/sample.csv.txt` — Per-cluster summary (human-readable)
- `outputs/sample.csv_metrics.txt` — Clustering evaluation scores

## License

MIT License. See [LICENSE](LICENSE) for details.
