# core/clustering.py
import time
import os
import re
import torch
import numpy as np
import pandas as pd
from urllib.parse import urlparse, unquote
from transformers import BertTokenizer, BertModel
from sklearn.preprocessing import normalize, OneHotEncoder, MinMaxScaler
from sklearn.feature_extraction.text import TfidfVectorizer
from custom_bkm import VerboseBisectingKMeans, flush_print
from sklearn.metrics import silhouette_score, davies_bouldin_score, calinski_harabasz_score
from decoder import parse_dec_file_to_dataframe

# Select GPU if available, otherwise fallback to CPU
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

# Load tokenizer and model ONCE globally for efficiency
TOKENIZER = BertTokenizer.from_pretrained("bert-base-uncased")
MODEL = BertModel.from_pretrained("bert-base-uncased").to(device)
MODEL.eval()

def mask_numbers(url):
    return re.sub(r'\d+', '<NUM>', url)

def split_url_tokens(url):
    parsed = urlparse(url)
    path = unquote(parsed.path)
    query = unquote(parsed.query)
    delimiters = r"[\/\-\_\=\&\?\.\+\(\)\[\]\<\>\{\}]"
    tokens = re.split(delimiters, path.strip("/")) + re.split(delimiters, query)
    return [tok for tok in tokens if tok]

def tokenize_user_agent(ua):
    tokens = re.split(r"[ /;()]+", ua)
    return [tok for tok in tokens if tok]

def categorize_status(code):
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

def generate_url_embeddings(url_list, batch_size=16):
    embeddings = []
    for i in range(0, len(url_list), batch_size):
        batch = url_list[i:i+batch_size]
        inputs = TOKENIZER(batch, return_tensors="pt", padding=True, truncation=True, max_length=64).to(device)
        with torch.no_grad():
            outputs = MODEL(**inputs)
        batch_emb = outputs.last_hidden_state.mean(dim=1)
        embeddings.append(batch_emb.cpu())
    return torch.cat(embeddings, dim=0).numpy()

def encode_methods(methods):
    return OneHotEncoder(sparse_output=False).fit_transform(
        np.array(methods).reshape(-1, 1)
    )

def encode_statuses(status_categories):
    return OneHotEncoder(sparse_output=False).fit_transform(
        np.array(status_categories).reshape(-1, 1)
    )

def normalize_sizes(sizes):
    return MinMaxScaler().fit_transform(np.array(sizes).reshape(-1, 1))

def vectorize_user_agents(ua_tokens, max_features=200):
    vectorizer = TfidfVectorizer(max_features=max_features)
    return vectorizer.fit_transform(ua_tokens).toarray()

def evaluate_clusters(features, labels):
    n_labels = len(set(labels))
    if n_labels < 2:
        return {"silhouette": None, "davies_bouldin": None, "calinski_harabasz": None}
    results = {}
    results["silhouette"] = silhouette_score(features, labels)
    results["davies_bouldin"] = davies_bouldin_score(features, labels)
    results["calinski_harabasz"] = calinski_harabasz_score(features, labels)
    return results

def cluster_logs(df, features, out_path, n_clusters):
    bkm = VerboseBisectingKMeans(n_clusters=n_clusters, random_state=42, init="k-means++", n_init=5)
    bkm.fit_verbose(features)
    labels = bkm.labels_

    df_label = df.copy()
    df_label["cluster"] = labels

    with open(f"{out_path}.txt", "w", encoding="utf-8") as f:
        for cluster_id in range(n_clusters):
            f.write(f"\nCluster {cluster_id}:\n")
            sample_rows = df_label[df_label["cluster"] == cluster_id]
            for _, row in sample_rows.iterrows():
                f.write(f"  {row['method']} {row['url']} [{row['status']}]\n")

    df_label.to_csv(out_path, index=False, encoding="utf-8")

    metrics = evaluate_clusters(features, labels)
    with open(f"{out_path}_metrics.txt", "w") as mf:
        for k, v in metrics.items():
            mf.write(f"{k}: {v}\n")

def run_clustering(input_path: str, output_path: str, n_clusters: int):

    flush_print(f"STATUS: Membaca data dari {input_path}")
    flush_print("PROGRESS: 1")

    df = parse_dec_file_to_dataframe(input_path)
    flush_print("STATUS: Membaca & parsing log selesai")
    flush_print("PROGRESS: 2")

    flush_print("STATUS: Ekstraksi & tokenisasi fitur...")
    tokenized_urls = [" ".join(split_url_tokens(mask_numbers(url))) for url in df['url']]
    methods = df['method'].tolist()
    status_categories = df['status'].apply(categorize_status).tolist()
    sizes = df['size'].tolist()
    ua_tokens = [" ".join(tokenize_user_agent(ua)) for ua in df['user_agent']]
    flush_print("STATUS: Ekstraksi fitur selesai")
    flush_print("PROGRESS: 3")

    flush_print("STATUS: Membuat embedding URL (BERT)...")
    url_embeddings = generate_url_embeddings(tokenized_urls)
    url_embeddings = normalize(url_embeddings)
    flush_print("STATUS: Embedding URL selesai")
    flush_print("PROGRESS: 4")

    flush_print("STATUS: Vektorisasi fitur lain...")
    method_enc = encode_methods(methods)
    status_enc = encode_statuses(status_categories)
    size_enc = normalize_sizes(sizes)
    ua_enc = vectorize_user_agents(ua_tokens)
    flush_print("STATUS: Vektorisasi fitur selesai")
    flush_print("PROGRESS: 5")

    final_features = np.hstack([url_embeddings, method_enc, status_enc, size_enc, ua_enc])

    flush_print("STATUS: Proses clustering Bisecting KMeans...")
    cluster_logs(df, final_features, output_path, n_clusters)
    flush_print(f"STATUS: Hasil disimpan ke {output_path}")
    flush_print("PROGRESS: 6")

