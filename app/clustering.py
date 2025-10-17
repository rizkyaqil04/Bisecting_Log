# core/clustering.py
import re
import os
import torch
import numpy as np
from urllib.parse import urlparse, unquote
from transformers import BertTokenizer, BertModel
# from gensim.models import FastText
from sklearn.cluster import KMeans
from custom_bkm import VerboseBisectingKMeans
from progress import ProgressManager, silence_output
from sklearn.preprocessing import normalize, OneHotEncoder, MinMaxScaler
from sklearn.metrics import silhouette_score, davies_bouldin_score, calinski_harabasz_score
import matplotlib.pyplot as plt
from sklearn.decomposition import PCA
from decoder import parse_dec_file_to_dataframe
from pprint import pprint
from tqdm import trange
from sklearn.feature_extraction.text import HashingVectorizer, TfidfVectorizer, TfidfTransformer
from scipy.sparse import csr_matrix, hstack, vstack, issparse
import math

# Select GPU if available, otherwise fallback to CPU
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

# Remove eager global loading of tokenizer/model to avoid noisy output at import time.
# TOKENIZER = BertTokenizer.from_pretrained("bert-base-uncased")
# MODEL = BertModel.from_pretrained("bert-base-uncased").to(device)
# MODEL.eval()

# ===================================================
# Functions for feature extraction and preprocessing
# ===================================================
def mask_numbers(url):
    """
    Replace numeric sequences in a URL with a placeholder token.

    Args:
        url (str): Input URL possibly containing numeric substrings.

    Returns:
        str: URL with numeric sequences replaced by '<NUM>'.
    """
    return re.sub(r'\d+', '<NUM>', url)


def split_url_tokens(url):
    """
    Split a URL into tokens using path and query delimiters.

    Args:
        url (str): Input URL to tokenize.

    Returns:
        list: List of non-empty token strings extracted from path and query.
    """
    parsed = urlparse(url)
    path = unquote(parsed.path)
    query = unquote(parsed.query)
    delimiters = r"[\/\-\_\=\&\?\.\+\(\)\[\]\<\>\{\}]"
    tokens = re.split(delimiters, path.strip("/")) + re.split(delimiters, query)
    return [tok for tok in tokens if tok]


def tokenize_user_agent(ua):
    """
    Tokenize a User-Agent string by common separators.

    Args:
        ua (str): User-Agent header string.

    Returns:
        list: Tokens representing browser/OS/engine identifiers.
    """
    tokens = re.split(r"[ /;()]+", ua)
    return [tok for tok in tokens if tok]


def categorize_status(code):
    """
    Map an HTTP status code to a status-category string.

    Args:
        code (int): HTTP status code.

    Returns:
        str: One of "2xx", "3xx", "4xx", "5xx", or "other".
    """
    if 200 <= code < 300:
        return "2xx"
    elif 300 <= code < 400:
        return "3xx"
    elif 400 <= code < 500:
        return "4xx"
    elif 500 <= code < 600:
        return "5xx"
    else:
        return "other"


# ===================================================
# Vectorization and embedding functions
# ===================================================
def generate_url_bert(url_list, TOKENIZER, MODEL, device, batch_size=32, out_path=None):
    """
    Generate BERT embeddings for a list of URL strings.

    Args:
        url_list (list[str]): Preprocessed URL strings.
        TOKENIZER: Transformers tokenizer instance.
        MODEL: Transformers model instance.
        device (torch.device): Device to run model on.
        batch_size (int): Batch size for embedding extraction.
        out_path (str|None): Optional memmap path to store embeddings.

    Returns:
        np.ndarray or np.memmap: Array of embeddings with dtype float32.
    """
    MODEL.eval()
    dim = MODEL.config.hidden_size

    if out_path:
        fp = np.memmap(out_path, dtype=np.float32, mode='w+', shape=(len(url_list), dim))
    else:
        fp = []

    with silence_output():
        for i in trange(0, len(url_list), batch_size, desc="Embedding URLs"):
            batch = url_list[i:i + batch_size]
            inputs = TOKENIZER(batch, return_tensors="pt", padding=True, truncation=True, max_length=64).to(device)
            with torch.no_grad():
                outputs = MODEL(**inputs)
            emb = outputs.last_hidden_state.mean(dim=1).cpu().numpy().astype(np.float32)

            if out_path:
                fp[i:i + len(batch)] = emb
            else:
                fp.append(emb)

            del emb, inputs, outputs
            try:
                torch.cuda.empty_cache()
            except Exception:
                pass

    if out_path:
        del fp
        return np.memmap(out_path, dtype=np.float32, mode='r', shape=(len(url_list), dim))
    else:
        return np.vstack(fp)
    
    
def generate_url_hashing(url_list, n_features=1024, batch_size=50000):
    """
    Generate feature vectors for URLs using a hashing trick.

    Args:
        url_list (list): List of tokenized URL strings.
        n_features (int, optional): Number of output features. Defaults to 1024.
        batch_size (int, optional): Number of URLs per batch. Defaults to 50000.

    Returns:
        scipy.sparse.csr_matrix: L2-normalized sparse matrix of hashed URL features.
    """
    vectorizer = HashingVectorizer(
        n_features=n_features,
        alternate_sign=False,
        dtype=np.float32
    )

    X_batches = []
    for i in range(0, len(url_list), batch_size):
        batch = url_list[i:i + batch_size]
        X_batches.append(vectorizer.transform(batch))
    X = vstack(X_batches)
    return normalize(X, norm='l2', copy=False)


def encode_methods(methods):
    """
    One-hot encode HTTP methods into a sparse matrix.

    Args:
        methods (list[str]): HTTP method strings (e.g., "GET", "POST").

    Returns:
        scipy.sparse.csr_matrix: One-hot encoded matrix (dtype=float32).
    """
    enc = OneHotEncoder(sparse_output=True, dtype=np.float32)
    return enc.fit_transform(np.array(methods).reshape(-1, 1))


def encode_statuses(status_categories):
    """
    One-hot encode status category labels into a sparse matrix.

    Args:
        status_categories (list[str]): Status category labels (e.g., "2xx").

    Returns:
        scipy.sparse.csr_matrix: One-hot encoded matrix (dtype=float32).
    """
    enc = OneHotEncoder(sparse_output=True, dtype=np.float32)
    return enc.fit_transform(np.array(status_categories).reshape(-1, 1))


def normalize_sizes(sizes):
    """
    Scale response sizes into the [0,1] range and return sparse matrix.

    Args:
        sizes (list[int|float]): Numeric response sizes.

    Returns:
        scipy.sparse.csr_matrix: Column matrix of normalized sizes (float32).
    """
    scaler = MinMaxScaler()
    arr = scaler.fit_transform(np.array(sizes, dtype=np.float32).reshape(-1, 1))
    return csr_matrix(arr)


def vectorize_user_agents(ua_tokens, max_features=200):
    """
    Convert tokenized user-agent strings into TF-IDF features.

    Args:
        ua_tokens (list[str]): Tokenized User-Agent text per record.
        max_features (int): Maximum TF-IDF vocabulary size.

    Returns:
        scipy.sparse.csr_matrix: TF-IDF feature matrix (float32).
    """
    vectorizer = TfidfVectorizer(max_features=max_features, dtype=np.float32)
    return vectorizer.fit_transform(ua_tokens)


# ===================================================
# Utility functions for combining features
# ===================================================
def combine_features(*arrays):
    """
    Combine multiple feature matrices (sparse or dense) into a single matrix.

    Args:
        *arrays: Variable number of feature matrices (np.ndarray or scipy.sparse).

    Returns:
        scipy.sparse.csr_matrix or np.ndarray: Stacked feature matrix.
    """
    arrays = [a for a in arrays if a is not None]
    if any(issparse(a) for a in arrays):
        arrays = [csr_matrix(a) if not issparse(a) else a for a in arrays]
        return hstack(arrays)
    return np.hstack(arrays)


# ===================================================
# Utility functions for clustering
# ===================================================
def evaluate_clusters(features, labels):
    """
    Evaluate clustering using silhouette, Davies–Bouldin and Calinski–Harabasz.

    Args:
        features (np.ndarray or scipy.sparse): Feature matrix used for evaluation.
        labels (np.ndarray): Cluster labels for each sample.

    Returns:
        dict: Mapping of metric name to value or None if skipped.
    """
    n_labels = len(set(labels))
    if n_labels < 2:
        return {
            "silhouette": None,
            "davies_bouldin": None,
            "calinski_harabasz": None
        }

    results = {}

    # Silhouette score
    try:
        results["silhouette"] = silhouette_score(features, labels, sample_size=20000 if len(labels) > 20000 else None)
    except Exception as e:
        results["silhouette"] = None
        print(f"Silhouette score skipped: {e}")

    # Davies–Bouldin and Calinski–Harabasz require dense data
    if issparse(features):
        if features.shape[0] * features.shape[1] < 50_000 * 1500:
            print("Converting sparse matrix to dense for small sample...")
            X_dense = features.toarray()
        else:
            print("Skipping Davies-Bouldin & Calinski-Harabasz (too large or sparse).")
            results["davies_bouldin"] = None
            results["calinski_harabasz"] = None
            return results
    else:
        X_dense = features

    # Compute remaining metrics safely
    try:
        results["davies_bouldin"] = davies_bouldin_score(X_dense, labels)
    except Exception as e:
        results["davies_bouldin"] = None
        print(f"Davies-Bouldin skipped: {e}")

    try:
        results["calinski_harabasz"] = calinski_harabasz_score(X_dense, labels)
    except Exception as e:
        results["calinski_harabasz"] = None
        print(f"Calinski-Harabasz skipped: {e}")

    return results


def bkm_fit(fit_features, n_clusters, progress_manager: ProgressManager = None):
    """
    Fit VerboseBisectingKMeans on provided features while reporting status/progress.

    Args:
        fit_features (np.ndarray or scipy.sparse): Feature matrix used for fitting.
        n_clusters (int): Desired number of clusters.
        progress_manager (ProgressManager|None): Shared progress manager for unified reporting.

    Returns:
        VerboseBisectingKMeans: Fitted bisecting KMeans model.
    """
    if progress_manager is None:
        # local ProgressManager so STATUS still shown via progress.py internals
        progress_manager = ProgressManager(4 + max(1, n_clusters - 1) + 3)

    progress_manager.set_status("Starting BisectingKMeans fitting")
    bkm = VerboseBisectingKMeans(
        n_clusters=n_clusters,
        random_state=42,
        init="k-means++",
        n_init=5
    )
    # VerboseBisectingKMeans.fit_verbose will advance progress_manager internally
    bkm.fit_verbose(fit_features, progress_manager=progress_manager)
    progress_manager.set_status("BisectingKMeans fit complete")
    return bkm


def predict_labels(model: VerboseBisectingKMeans, predict_features=None, fit_features=None, progress_manager: ProgressManager = None):
    """
    Produce cluster labels by predicting or using model-assigned labels.

    Args:
        model (VerboseBisectingKMeans): Fitted model instance.
        predict_features (np.ndarray or scipy.sparse|None): Optional features to predict on.
        fit_features: Original features used for fitting (fallback).
        progress_manager (ProgressManager|None): Shared progress manager.

    Returns:
        np.ndarray: Array of cluster labels.
    """
    if progress_manager is None:
        progress_manager = ProgressManager(1)

    progress_manager.set_status("Generating cluster labels (predict/assign)")
    if predict_features is not None:
        labels = model.predict(predict_features)
    else:
        labels = model.labels_
    progress_manager.set_status("Cluster labels ready")
    return labels


def save_cluster_outputs(df, labels, out_path, n_clusters, progress_manager: ProgressManager = None):
    """
    Save clustered DataFrame as compressed CSV and write a text summary.

    Args:
        df (pandas.DataFrame): Original dataframe to attach labels to.
        labels (np.ndarray): Cluster labels to attach.
        out_path (str): Output path base (extensions appended).
        n_clusters (int): Number of clusters (used for summary).
        progress_manager (ProgressManager|None): Shared progress manager.

    Returns:
        None
    """
    if progress_manager is None:
        progress_manager = ProgressManager(1)

    progress_manager.set_status("Attaching labels to DataFrame")
    df_label = df.copy()
    df_label["cluster"] = labels

    base, _ = os.path.splitext(out_path)
    csv_path = f"{base}.csv.gz"
    txt_path = f"{base}.txt"

    progress_manager.set_status(f"Writing compressed CSV to: {csv_path}")
    chunk_size = 100_000
    with open(csv_path, "wb") as f:
        import gzip
        with gzip.open(f, "wt", encoding="utf-8", newline="") as gz:
            df_label.head(0).to_csv(gz, index=False)
            for i in range(0, len(df_label), chunk_size):
                df_label.iloc[i:i+chunk_size].to_csv(gz, header=False, index=False)

    progress_manager.set_status(f"Compressed CSV saved: {csv_path}")

    progress_manager.set_status(f"Writing text summary: {txt_path}")
    max_per_cluster = 1000 if len(df_label) > 1_000_000 else None
    with open(txt_path, "w", encoding="utf-8") as f:
        for cluster_id in range(n_clusters):
            cluster_data = df_label[df_label["cluster"] == cluster_id]
            f.write(f"\nCluster {cluster_id} ({len(cluster_data)} entries):\n")
            if max_per_cluster:
                cluster_data = cluster_data.head(max_per_cluster)
                f.write(f"  [Showing first {max_per_cluster} entries]\n")
            for _, row in cluster_data.iterrows():
                f.write(f"  {row['method']} {row['url']} [{row['status']}]\n")

    progress_manager.set_status(f"Cluster summaries saved: {txt_path}")
    return


def evaluate_and_save_metrics(fit_features, labels, out_path, progress_manager: ProgressManager = None):
    """
    Evaluate clustering metrics (with sampling for large datasets) and save to file.

    Args:
        fit_features (np.ndarray or scipy.sparse): Features used for evaluation.
        labels (np.ndarray): Cluster labels corresponding to fit_features.
        out_path (str): Output path base to write metrics file.
        progress_manager (ProgressManager|None): Shared progress manager.

    Returns:
        dict: Computed metrics mapping names to values.
    """
    if progress_manager is None:
        progress_manager = ProgressManager(1)

    progress_manager.set_status("Evaluating clustering metrics")
    # sample if large
    if (hasattr(fit_features, "shape") and fit_features.shape[0] > 200_000) or (isinstance(fit_features, (list, np.ndarray)) and len(fit_features) > 200_000):
        progress_manager.set_status("Large dataset detected, evaluating on sample of 200k entries...")
        idx = np.random.choice(fit_features.shape[0], 200_000, replace=False)
        sample_features = fit_features[idx] if not issparse(fit_features) else fit_features[idx, :]
        sample_labels = labels[idx]
        metrics = evaluate_clusters(sample_features, sample_labels)
    else:
        metrics = evaluate_clusters(fit_features, labels)

    base, _ = os.path.splitext(out_path)
    metrics_path = f"{base}_metrics.txt"
    with open(metrics_path, "w") as mf:
        for k, v in metrics.items():
            mf.write(f"{k}: {v}\n")

    progress_manager.set_status(f"Metrics saved: {metrics_path}")
    return metrics


# ===================================================
# Clustering pipeline function
# ===================================================
def run_clustering(input_path: str, output_path: str, n_clusters: int, sample_frac: float = 0.2):
    """
    End-to-end clustering pipeline: read, preprocess, embed, vectorize, cluster and evaluate.

    Args:
        input_path (str): Path to input decoded log file.
        output_path (str): Path to write resulting CSV (and additional text files).
        n_clusters (int): Number of clusters to produce.
        sample_frac (float): Fraction sampled for model fitting (default: 0.2).

    Returns:
        None
    """
    # Define broad pipeline stages
    stages = [
        "read_and_parse",
        "extract_and_tokenize",
        "embed_urls",
        "vectorize_other_features",
        "combine_and_sample",
        "fit_model_on_sample",
        "assign_and_save",
        "evaluate_and_finish",
    ]

    # Compute BisectingKMeans internal units (must match custom_bkm logic)
    bkm_units = 4 + max(1, n_clusters - 1) + 3

    # Per-stage atomic unit allocation: most stages 1 unit; KMeans stage gets bkm_units
    stage_units = {s: (bkm_units if s == "fit_model_on_sample" else 1) for s in stages}
    total_units = sum(stage_units.values())

    # Initialize shared progress manager (single unified PROGRESS)
    progress = ProgressManager(total_units)
    progress.set_status("Initializing pipeline")

    # Stage 0: read & parse
    progress.set_status(f"Reading and parsing data from {input_path}. It will take some time...")
    df = parse_dec_file_to_dataframe(input_path)
    progress.advance(stage_units["read_and_parse"])

    # Stage 1: extract & tokenize features
    progress.set_status("Extracting and tokenizing features (URL, method, status, size, user-agent)")
    tokenized_urls = [" ".join(split_url_tokens(mask_numbers(url))) for url in df['url']]
    methods = df['method'].tolist()
    status_categories = df['status'].apply(categorize_status).tolist()
    sizes = df['size'].tolist()
    ua_tokens = [" ".join(tokenize_user_agent(ua)) for ua in df['user_agent']]
    progress.advance(stage_units["extract_and_tokenize"])

    # Stage 2: generate URL embeddings
    progress.set_status("Generating URL embeddings using BERT. It will take some time...")
    with silence_output():
        TOKENIZER = BertTokenizer.from_pretrained("sentence-transformers/all-MiniLM-L6-v2")
        MODEL = BertModel.from_pretrained("sentence-transformers/all-MiniLM-L6-v2").to(device)
        MODEL.eval()

    url_embeddings = generate_url_bert(tokenized_urls, TOKENIZER, MODEL, device)
    url_embeddings = normalize(url_embeddings)
    progress.advance(stage_units["embed_urls"])

    # Stage 3: vectorize other features
    progress.set_status("Vectorizing HTTP methods, status categories, sizes, and user-agents")
    method_enc = encode_methods(methods)
    status_enc = encode_statuses(status_categories)
    size_enc = normalize_sizes(sizes)
    ua_enc = vectorize_user_agents(ua_tokens)
    progress.advance(stage_units["vectorize_other_features"])

    # Stage 4: combine features and optionally sample
    progress.set_status("Combining feature matrices and preparing sampling")
    final_features = combine_features(url_embeddings, method_enc, status_enc, size_enc, ua_enc)

    if sample_frac < 1.0:
        progress.set_status(f"Sampling {sample_frac*100:.0f}% of data for clustering training")
        df_sample = df.sample(frac=sample_frac, random_state=42)
        features_sample = final_features[df_sample.index]
    else:
        df_sample = df
        features_sample = final_features
    progress.advance(stage_units["combine_and_sample"])

    # Stage 5: fit model on sample
    progress.set_status("Fitting bisecting KMeans on sampled data. It will take some time...")
    bkm = bkm_fit(features_sample, n_clusters, progress_manager=progress)

    progress.set_status("Predicting cluster labels for full dataset. It will take some time...")
    labels = predict_labels(bkm, predict_features=final_features, fit_features=features_sample, progress_manager=progress)

    # Stage 6: save outputs
    progress.set_status("Saving clustered outputs (CSV + TXT). It will take some time...")
    save_cluster_outputs(df, labels, output_path, n_clusters, progress_manager=progress)
    progress.advance(stage_units["assign_and_save"])

    # Stage 7: evaluate & save metrics
    progress.set_status("Evaluating clustering (metrics)")
    evaluate_and_save_metrics(features_sample, labels, output_path, progress_manager=progress)
    progress.advance(stage_units["evaluate_and_finish"])

    # Finalize
    progress.set_status(f"Results saved to {output_path}")
    progress.complete()

