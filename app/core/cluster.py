# app/core/cluster.py

import os
import logging
import numpy as np
from scipy.sparse import issparse

from .embeddings import generate_url_bert, get_cache_dir
from .features import (
    mask_numbers,
    split_url_tokens,
    tokenize_user_agent,
    categorize_status,
    encode_methods,
    encode_statuses,
    normalize_sizes,
    vectorize_user_agents,
    combine_features,
)
from .custom_bkm import VerboseBisectingKMeans
from .progress import ProgressManager, silence_output
from .decoder import parse_dec_file_to_dataframe


def evaluate_clusters(features, labels):
    n_labels = len(set(labels))
    if n_labels < 2:
        return {"silhouette": None, "davies_bouldin": None, "calinski_harabasz": None}

    results = {}
    try:
        from sklearn.metrics import silhouette_score, davies_bouldin_score, calinski_harabasz_score
    except Exception:
        return {"silhouette": None, "davies_bouldin": None, "calinski_harabasz": None}

    try:
        results["silhouette"] = silhouette_score(features, labels, sample_size=20000 if len(labels) > 20000 else None)
    except Exception as e:
        results["silhouette"] = None
        print(f"Silhouette score skipped: {e}")

    if issparse(features):
        if features.shape[0] * features.shape[1] < 50_000 * 1500:
            X_dense = features.toarray()
        else:
            print("Skipping Davies-Bouldin & Calinski-Harabasz (too large or sparse).")
            results["davies_bouldin"] = None
            results["calinski_harabasz"] = None
            return results
    else:
        X_dense = features

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
    if progress_manager is None:
        progress_manager = ProgressManager(4 + max(1, n_clusters - 1) + 3)

    progress_manager.set_status("Starting BisectingKMeans fitting")
    bkm = VerboseBisectingKMeans(n_clusters=n_clusters, random_state=42, init="k-means++", n_init=5)
    features_to_fit = fit_features
    if isinstance(fit_features, tuple) and isinstance(fit_features[0], np.ndarray):
        features_to_fit = fit_features[0]

    bkm.fit_verbose(features_to_fit, progress_manager=progress_manager)
    progress_manager.set_status("BisectingKMeans fit complete")
    return bkm


def predict_labels(model: VerboseBisectingKMeans, predict_features=None, fit_features=None, progress_manager: ProgressManager = None):
    if progress_manager is None:
        progress_manager = ProgressManager(1)

    progress_manager.set_status("Generating cluster labels (predict/assign)")
    if predict_features is not None:
        if isinstance(predict_features, tuple) and isinstance(predict_features[0], np.ndarray):
            labels = model.predict(predict_features[0])
        else:
            labels = model.predict(predict_features)
    else:
        labels = model.labels_
    progress_manager.set_status("Cluster labels ready")
    return labels


def _strip_known_extensions(path: str) -> str:
    base = path
    if base.lower().endswith('.gz'):
        base = base[:-3]
    while True:
        base_no_ext, ext = os.path.splitext(base)
        if ext.lower() in ('.csv', '.txt', '.json', '.log'):
            base = base_no_ext
            continue
        break
    return base


def save_cluster_outputs(df, labels, out_path, n_clusters, progress_manager: ProgressManager = None):
    if progress_manager is None:
        progress_manager = ProgressManager(1)

    progress_manager.set_status("Attaching labels to DataFrame")
    df_label = df.copy()
    df_label["cluster"] = labels

    base = _strip_known_extensions(out_path)
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
    if progress_manager is None:
        progress_manager = ProgressManager(1)

    progress_manager.set_status("Evaluating clustering metrics")
    if isinstance(fit_features, tuple) and isinstance(fit_features[0], np.ndarray):
        dense_features = fit_features[0]
    else:
        dense_features = fit_features

    if (hasattr(dense_features, "shape") and dense_features.shape[0] > 200_000) or (isinstance(dense_features, (list, np.ndarray)) and len(dense_features) > 200_000):
        progress_manager.set_status("Large dataset detected, evaluating on sample of 200k entries...")
        idx = np.random.choice(dense_features.shape[0], 200_000, replace=False)
        sample_features = dense_features[idx] if not issparse(dense_features) else dense_features[idx, :]
        sample_labels = labels[idx]
        metrics = evaluate_clusters(sample_features, sample_labels)
    else:
        metrics = evaluate_clusters(dense_features, labels)

    base_output = _strip_known_extensions(out_path)
    metrics_path = f"{base_output}_metrics.txt"
    with open(metrics_path, "w") as mf:
        for k, v in metrics.items():
            mf.write(f"{k}: {v}\n")

    progress_manager.set_status(f"Metrics saved: {metrics_path}")
    return metrics


def run_clustering(input_path: str, output_path: str, n_clusters: int, sample_frac: float = 0.2, force_embed: bool = False, keep_f32: bool = False, max_cache_rows: int | None = None):
    logger = logging.getLogger("bisecting_log")
    logger.setLevel(logging.INFO)
    logger.handlers = []
    logger.propagate = False
    base_output = _strip_known_extensions(output_path)
    log_path = f"{base_output}_status.log"
    fh = logging.FileHandler(log_path, mode="a", encoding="utf-8")
    fh.setFormatter(logging.Formatter("%(asctime)s %(levelname)s: %(message)s"))
    logger.addHandler(fh)

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

    bkm_units = 4 + max(1, n_clusters - 1) + 3
    stage_units = {s: (bkm_units if s == "fit_model_on_sample" else 1) for s in stages}
    total_units = sum(stage_units.values())

    progress = ProgressManager(total_units)

    _orig_set_status = progress.set_status

    def _logged_set_status(msg):
        try:
            _orig_set_status(msg)
        finally:
            logger.info(msg)

    progress.set_status = _logged_set_status

    progress.set_status("Initializing pipeline")

    progress.set_status(f"Reading and parsing data from {input_path}. It will take some time...")
    df = parse_dec_file_to_dataframe(input_path)
    progress.advance(stage_units["read_and_parse"])

    progress.set_status("Extracting and tokenizing features (URL, method, status, size, user-agent)")
    tokenized_urls = [" ".join(split_url_tokens(mask_numbers(url))) for url in df['url']]
    methods = df['method'].tolist()
    status_categories = df['status'].apply(categorize_status).tolist()
    sizes = df['size'].tolist()
    ua_tokens = [" ".join(tokenize_user_agent(ua)) for ua in df['user_agent']]
    progress.advance(stage_units["extract_and_tokenize"])

    progress.set_status("Generating URL embeddings using BERT. It will take some time...")
    with silence_output():
        from transformers import BertTokenizer, BertModel
        TOKENIZER = BertTokenizer.from_pretrained("sentence-transformers/all-MiniLM-L6-v2")
        MODEL = BertModel.from_pretrained("sentence-transformers/all-MiniLM-L6-v2").to('cuda' if __import__('torch').cuda.is_available() else 'cpu')
        MODEL.eval()

    url_embeddings = generate_url_bert(tokenized_urls, TOKENIZER, MODEL, MODEL.device if hasattr(MODEL, 'device') else None, batch_size=32, out_path="embeddings", force_embed=force_embed, max_cache_rows=max_cache_rows)

    try:
        if getattr(url_embeddings, "dtype", None) == np.float16:
            if isinstance(url_embeddings, np.memmap):
                cache_embed_dir = get_cache_dir("embeddings")
                f32_path = os.path.join(cache_embed_dir, "url_embeddings_f32.memmap")
                n_rows = url_embeddings.shape[0]
                emb_dim = url_embeddings.shape[1]
                try:
                    if os.path.exists(f32_path):
                        mm_test = np.memmap(f32_path, dtype=np.float32, mode='r', shape=(n_rows, emb_dim))
                        url_embeddings = mm_test
                    else:
                        chunk = get_cache_dir  # placeholder to keep simple (already computed in embeddings module when needed)
                        mm_f32 = np.memmap(f32_path, dtype=np.float32, mode='w+', shape=(n_rows, emb_dim))
                        mm_f32[:] = url_embeddings.astype(np.float32)
                        try:
                            del mm_f32
                        except Exception:
                            pass
                        url_embeddings = np.memmap(f32_path, dtype=np.float32, mode='r', shape=(n_rows, emb_dim))
                except Exception:
                    url_embeddings = url_embeddings.astype(np.float32)
            else:
                url_embeddings = url_embeddings.astype(np.float32)
    except Exception:
        pass

    try:
        if getattr(url_embeddings, "dtype", None) == np.float32 and isinstance(url_embeddings, np.memmap):
            cache_embed_dir = get_cache_dir("embeddings")
            norm_path = os.path.join(cache_embed_dir, "url_embeddings_f32_norm.memmap")
            n_rows, emb_dim = url_embeddings.shape
            if os.path.exists(norm_path):
                try:
                    url_embeddings = np.memmap(norm_path, dtype=np.float32, mode='r', shape=(n_rows, emb_dim))
                except Exception:
                    pass

            if not isinstance(url_embeddings, np.memmap) or (isinstance(url_embeddings, np.memmap) and not url_embeddings.filename.endswith("url_embeddings_f32_norm.memmap")):
                mm_norm = np.memmap(norm_path, dtype=np.float32, mode='w+', shape=(n_rows, emb_dim))
                src_path = os.path.join(get_cache_dir("embeddings"), "url_embeddings_f32.memmap")
                src = np.memmap(src_path, dtype=np.float32, mode='r', shape=(n_rows, emb_dim))
                chunk = 8192
                for start in range(0, n_rows, chunk):
                    end = min(start + chunk, n_rows)
                    block = src[start:end]
                    norms = np.linalg.norm(block, axis=1, keepdims=True).astype(np.float32)
                    mm_norm[start:end] = block / (norms + 1e-8)
                try:
                    del mm_norm
                except Exception:
                    pass
                url_embeddings = np.memmap(norm_path, dtype=np.float32, mode='r', shape=(n_rows, emb_dim))
        else:
            arr = np.asarray(url_embeddings, dtype=np.float32)
            norms = np.linalg.norm(arr, axis=1, keepdims=True).astype(np.float32)
            url_embeddings = arr / (norms + 1e-8)
    except Exception:
        try:
            from sklearn.preprocessing import normalize
            url_embeddings = normalize(url_embeddings)
        except Exception:
            pass
    progress.advance(stage_units["embed_urls"])

    progress.set_status("Vectorizing HTTP methods, status categories, sizes, and user-agents")
    method_enc = encode_methods(methods)
    status_enc = encode_statuses(status_categories)
    size_enc = normalize_sizes(sizes)
    ua_enc = vectorize_user_agents(ua_tokens, out_path="ua_features")
    progress.advance(stage_units["vectorize_other_features"])

    progress.set_status("Combining feature matrices and preparing sampling")
    final_features = combine_features(url_embeddings, method_enc, status_enc, size_enc, ua_enc)

    if len(df) > 200000 and sample_frac < 1.0:
        df_sample = df.sample(frac=sample_frac, random_state=42)
        idx = np.sort(df_sample.index.values)
        features_sample = final_features[idx]
    else:
        df_sample = df
        features_sample = final_features
    progress.advance(stage_units["combine_and_sample"])

    progress.set_status("Fitting bisecting KMeans on sampled data. It will take some time...")
    bkm = bkm_fit(features_sample, n_clusters, progress_manager=progress)

    progress.set_status("Predicting cluster labels for full dataset. It will take some time...")
    labels = predict_labels(bkm, predict_features=final_features, fit_features=features_sample, progress_manager=progress)

    progress.set_status(f"Saving clustered outputs (CSV + TXT). It will take some time...")
    save_cluster_outputs(df, labels, output_path, n_clusters, progress_manager=progress)
    progress.advance(stage_units["assign_and_save"])

    progress.set_status("Evaluating clustering (metrics)")
    evaluate_and_save_metrics(features_sample, labels, output_path, progress_manager=progress)
    progress.advance(stage_units["evaluate_and_finish"])

    progress.set_status(f"Results saved to {output_path}")
    progress.complete()

    for h in logger.handlers:
        try:
            h.flush()
            h.close()
        except Exception:
            pass
    logger.handlers = []
