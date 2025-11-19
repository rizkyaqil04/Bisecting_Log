# app/core/features.py

import os
import joblib
import numpy as np
from urllib.parse import urlparse, unquote
import re
from sklearn.preprocessing import OneHotEncoder, MinMaxScaler
from sklearn.feature_extraction.text import TfidfVectorizer
from scipy.sparse import csr_matrix, hstack, issparse, diags

from .embeddings import get_cache_dir


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
    try:
        code = int(code)
    except Exception:
        return 'other'
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


def encode_methods(methods):
    enc = OneHotEncoder(sparse_output=True, dtype=np.float32)
    return enc.fit_transform(np.array(methods).reshape(-1, 1))


def encode_statuses(status_categories):
    enc = OneHotEncoder(sparse_output=True, dtype=np.float32)
    return enc.fit_transform(np.array(status_categories).reshape(-1, 1))


def normalize_sizes(sizes):
    scaler = MinMaxScaler()
    arr = scaler.fit_transform(np.array(sizes, dtype=np.float32).reshape(-1, 1))
    return csr_matrix(arr)


def vectorize_user_agents(ua_tokens, max_features=200, out_path=None):
    if not out_path:
        vectorizer = TfidfVectorizer(max_features=max_features, dtype=np.float32)
        return vectorizer.fit_transform(ua_tokens)

    cache_feat_dir = get_cache_dir(out_path)
    vocab_path = os.path.join(cache_feat_dir, "ua_tfidf_vocab.pkl")

    if os.path.exists(vocab_path):
        data = joblib.load(vocab_path)
        vectorizer = TfidfVectorizer(dtype=np.float32, vocabulary=data["vocab"])
        vectorizer.idf_ = data["idf"]
        vectorizer._tfidf._idf_diag = diags(data["idf"])
        return vectorizer.transform(ua_tokens)

    vectorizer = TfidfVectorizer(max_features=max_features, dtype=np.float32)
    X = vectorizer.fit_transform(ua_tokens)

    joblib.dump({"vocab": vectorizer.vocabulary_, "idf": vectorizer.idf_}, vocab_path)

    return X


def combine_features(*arrays):
    arrays = [a for a in arrays if a is not None]
    if not arrays:
        return None

    first = arrays[0]
    rest = arrays[1:]
    if isinstance(first, np.ndarray) and any(issparse(a) for a in rest):
        sparse_rest = [csr_matrix(a) if not issparse(a) else a for a in rest]
        sparse_h = hstack(sparse_rest) if sparse_rest else None
        return (first, sparse_h)

    if any(issparse(a) for a in arrays):
        arrays = [csr_matrix(a) if not issparse(a) else a for a in arrays]
        return hstack(arrays)

    return np.hstack(arrays)
