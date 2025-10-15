# core/run.py
import argparse
from clustering import run_clustering

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="KMeans verbose")
    parser.add_argument("--input", type=str, required=True, help="Path file log input")
    parser.add_argument("--output", type=str, required=True, help="Path hasil CSV")
    parser.add_argument("--n", type=int, default=8, help="Number of clusters (default: 8)")
    args = parser.parse_args()

    run_clustering(args.input, args.output, args.n)
