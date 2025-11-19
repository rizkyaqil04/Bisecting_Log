# app/core/embeddings.py

import os
import shutil
import joblib
import logging
import numpy as np
import torch
from tqdm import trange


def get_adaptive_chunk(rows, emb_dim, dtype_bytes=4, frac=0.25, min_chunk=256, max_chunk=65536):
    try:
        with open('/proc/meminfo', 'r') as f:
            meminfo = f.read()
        for line in meminfo.splitlines():
            if line.startswith('MemAvailable:'):
                parts = line.split()
                avail_kb = int(parts[1])
                break
        else:
            avail_kb = 0
        avail_bytes = avail_kb * 1024
        target = int(avail_bytes * frac)
        if target <= 0:
            raise Exception('no mem')
        per_row = emb_dim * dtype_bytes
        chunk = max(min(int(target / per_row), max_chunk), min_chunk)
    except Exception:
        chunk = 8192
    return max(1, min(rows, chunk))


def get_cache_dir(subfolder=None):
    project_root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    cache_root = os.path.join(project_root, "cache")
    os.makedirs(cache_root, exist_ok=True)

    if subfolder:
        path = os.path.join(cache_root, subfolder)
        os.makedirs(path, exist_ok=True)
        return path

    return cache_root


def generate_url_bert(url_list, TOKENIZER, MODEL, device, batch_size=32, out_path="embeddings", force_embed=False, max_cache_rows=None):
    MODEL.eval()
    dim = MODEL.config.hidden_size

    if not out_path:
        fp = []
    else:
        cache_embed_dir = get_cache_dir(out_path)
        perrun_path = os.path.join(cache_embed_dir, "url_embeddings.memmap")
        global_path = os.path.join(cache_embed_dir, "global_url_embeddings.memmap")
        index_path = os.path.join(cache_embed_dir, "url_index.pkl")
        meta_path = os.path.join(cache_embed_dir, "global_meta.pkl")

        if force_embed:
            for p in (perrun_path, global_path, index_path, meta_path, os.path.join(cache_embed_dir, "url_embeddings_f32.memmap"), os.path.join(cache_embed_dir, "url_embeddings_f32_norm.memmap")):
                try:
                    if os.path.exists(p):
                        os.remove(p)
                except Exception:
                    pass

        index = {}
        meta = {"capacity": 0, "n_stored": 0, "dim": dim, "dtype": "float16"}
        if os.path.exists(index_path):
            try:
                index = joblib.load(index_path)
            except Exception:
                index = {}
        if os.path.exists(meta_path):
            try:
                meta = joblib.load(meta_path)
            except Exception:
                meta = {"capacity": 0, "n_stored": 0, "dim": dim, "dtype": "float16"}

        dtype = np.float16
        capacity = meta.get("capacity", 0)
        n_stored = meta.get("n_stored", 0)

        if capacity > 0 and os.path.exists(global_path):
            global_mm = np.memmap(global_path, dtype=dtype, mode='r+', shape=(capacity, dim))
        else:
            global_mm = None

        try:
            arr_urls = np.array(url_list, dtype=object)
            unique_urls_np, inv = np.unique(arr_urls, return_inverse=True)
            unique_urls = unique_urls_np.tolist()
            orig_to_unique = inv.tolist()
        except Exception:
            unique_urls = []
            orig_to_unique = []
            seen = {}
            for u in url_list:
                if u in seen:
                    orig_to_unique.append(seen[u])
                else:
                    idxu = len(unique_urls)
                    unique_urls.append(u)
                    seen[u] = idxu
                    orig_to_unique.append(idxu)

        urls_to_compute = []
        urls_to_compute_idx = []
        for i, u in enumerate(unique_urls):
            if u in index:
                continue
            urls_to_compute.append(u)
            urls_to_compute_idx.append(i)

        cache_hits = len(unique_urls) - len(urls_to_compute)
        cache_misses = len(urls_to_compute)
        logging.getLogger(__name__).info(f"URL embedding cache: {cache_hits} hits, {cache_misses} misses (unique={len(unique_urls)})")

        if urls_to_compute:
            need_extra = len(urls_to_compute)
            if capacity - n_stored < need_extra:
                new_capacity = max(capacity * 2 if capacity > 0 else 1024, capacity + need_extra)
                if max_cache_rows is not None:
                    if new_capacity > max_cache_rows:
                        if capacity >= max_cache_rows:
                            new_capacity = capacity
                        else:
                            new_capacity = max_cache_rows
                tmp_path = global_path + ".tmp"
                new_mm = np.memmap(tmp_path, dtype=dtype, mode='w+', shape=(new_capacity, dim))
                if global_mm is not None:
                    new_mm[:n_stored, :] = global_mm[:n_stored, :]
                try:
                    del global_mm
                except Exception:
                    pass
                try:
                    if os.path.exists(global_path):
                        os.remove(global_path)
                except Exception:
                    pass
                shutil.move(tmp_path, global_path)
                capacity = new_capacity
                meta["capacity"] = capacity
                global_mm = np.memmap(global_path, dtype=dtype, mode='r+', shape=(capacity, dim))

            for i in trange(0, len(urls_to_compute), batch_size, desc="Embedding missing URLs"):
                batch = urls_to_compute[i : i + batch_size]
                inputs = TOKENIZER(batch, return_tensors="pt", padding=True, truncation=True, max_length=64).to(device)
                with torch.no_grad():
                    if device.type == "cuda":
                        with torch.autocast(device_type="cuda", dtype=torch.float16):
                            outputs = MODEL(**inputs)
                    else:
                        outputs = MODEL(**inputs)
                emb = outputs.last_hidden_state.mean(dim=1).cpu().numpy().astype(np.float16)

                can_store_globally = (global_mm is not None) and (n_stored + emb.shape[0] <= capacity)
                if can_store_globally:
                    global_mm[n_stored : n_stored + emb.shape[0], :] = emb
                    for j, u_idx in enumerate(urls_to_compute_idx[i : i + batch_size]):
                        url_str = unique_urls[u_idx]
                        index[url_str] = n_stored + j
                    n_stored += emb.shape[0]
                else:
                    if 'temp_new_embeddings' not in locals():
                        temp_new_embeddings = {}
                    for j, u_idx in enumerate(urls_to_compute_idx[i : i + batch_size]):
                        url_str = unique_urls[u_idx]
                        temp_new_embeddings[url_str] = emb[j]

                del emb, inputs, outputs
                try:
                    torch.cuda.empty_cache()
                except Exception:
                    pass

            meta["n_stored"] = n_stored
            meta["dim"] = dim
            meta["dtype"] = 'float16'
            try:
                joblib.dump(index, index_path)
                joblib.dump(meta, meta_path)
            except Exception:
                pass

        perrun_mm = np.memmap(perrun_path, dtype=dtype, mode='w+', shape=(len(url_list), dim))
        for orig_idx, uniq_idx in enumerate(orig_to_unique):
            urlstr = unique_urls[uniq_idx]
            global_idx = index.get(urlstr)
            if global_idx is None:
                if 'temp_new_embeddings' in locals() and urlstr in temp_new_embeddings:
                    perrun_mm[orig_idx, :] = temp_new_embeddings[urlstr]
                else:
                    perrun_mm[orig_idx, :] = np.zeros((dim,), dtype=dtype)
            else:
                perrun_mm[orig_idx, :] = global_mm[global_idx, :]

        try:
            del perrun_mm
        except Exception:
            pass

        return np.memmap(perrun_path, dtype=dtype, mode='r', shape=(len(url_list), dim))

    for i in trange(0, len(url_list), batch_size, desc="Embedding URLs"):
        batch = url_list[i : i + batch_size]
        inputs = TOKENIZER(batch, return_tensors="pt", padding=True, truncation=True, max_length=64).to(device)

        with torch.no_grad():
            if device.type == "cuda":
                with torch.autocast(device_type="cuda", dtype=torch.float16):
                    outputs = MODEL(**inputs)
            else:
                outputs = MODEL(**inputs)

        emb = outputs.last_hidden_state.mean(dim=1).cpu().float().numpy()
        fp.append(emb)

        del emb, inputs, outputs
        try:
            torch.cuda.empty_cache()
        except:
            pass

    return np.vstack(fp)
