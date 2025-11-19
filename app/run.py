# app/run.py

import argparse
from core.cluster import run_clustering

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="KMeans verbose")
    parser.add_argument("--input", type=str, required=True, help="Path file log input")
    parser.add_argument("--output", type=str, required=True, help="Path hasil CSV")
    parser.add_argument("--n", type=int, default=8, help="Number of clusters (default: 8)")
    parser.add_argument("--force-embed", action="store_true", help="Force recompute embeddings and reset cache/index")
    parser.add_argument("--keep-f32", action="store_true", help="Keep FP32 memmaps after run (default: temp files will be created)")
    parser.add_argument("--max-cache-rows", type=int, default=None, help="Maximum number of rows to allow in global cache (FP16 rows).")
    args = parser.parse_args()

    run_clustering(args.input, args.output, args.n, force_embed=args.force_embed, keep_f32=args.keep_f32, max_cache_rows=args.max_cache_rows)
