# core/custom_bisecting_kmeans.py
import numpy as np
import scipy.sparse as sp
import warnings
from sklearn.cluster import BisectingKMeans
from sklearn.utils.extmath import row_norms
from sklearn.utils.validation import check_random_state, validate_data
from sklearn.utils._openmp_helpers import _openmp_effective_n_threads
from sklearn.utils.validation import _check_sample_weight
from sklearn.cluster._k_means_common import _inertia_dense, _inertia_sparse

def flush_print(msg: str):
    print(msg, flush=True)


class VerboseBisectingKMeans(BisectingKMeans):
    """
    Versi Bisecting KMeans dengan progress dan status real-time.
    Struktur dan perilaku mengikuti sklearn.cluster.BisectingKMeans.
    """

    def fit_verbose(self, X, y=None, sample_weight=None):
        # === 1. Validasi Input ===
        X = validate_data(
            self,
            X,
            accept_sparse="csr",
            dtype=[np.float64, np.float32],
            order="C",
            copy=self.copy_x,
            accept_large_sparse=False,
        )

        self._check_params_vs_input(X)
        self._random_state = check_random_state(self.random_state)
        sample_weight = _check_sample_weight(sample_weight, X, dtype=X.dtype)
        self._n_threads = _openmp_effective_n_threads()

        flush_print(f"STATUS: Memulai BisectingKMeans (k={self.n_clusters})")
        flush_print("PROGRESS: 8")

        # === 2. Setup algorithm (lloyd/elkan) ===
        if self.algorithm == "lloyd" or self.n_clusters == 1:
            from sklearn.cluster._kmeans import _kmeans_single_lloyd
            self._kmeans_single = _kmeans_single_lloyd
            self._check_mkl_vcomp(X, X.shape[0])
        else:
            from sklearn.cluster._kmeans import _kmeans_single_elkan
            self._kmeans_single = _kmeans_single_elkan

        # === 3. Centering data ===
        if not sp.issparse(X):
            self._X_mean = X.mean(axis=0)
            X -= self._X_mean
        x_squared_norms = row_norms(X, squared=True)

        # === 4. Buat root tree ===
        from sklearn.cluster._bisect_k_means import _BisectingTree
        self._bisecting_tree = _BisectingTree(
            indices=np.arange(X.shape[0]),
            center=X.mean(axis=0),
            score=0,
        )

        flush_print("STATUS: Root cluster dibuat")
        flush_print("PROGRESS: 10")

        # === 5. Iterasi bisection utama ===
        for step in range(self.n_clusters - 1):
            cluster_to_bisect = self._bisecting_tree.get_cluster_to_bisect()

            flush_print(f"STATUS: Membagi cluster ke-{step+1}/{self.n_clusters-1} (size={len(cluster_to_bisect.indices)})")
            self._bisect(X, x_squared_norms, sample_weight, cluster_to_bisect)

            progress = int(5 + (step + 1) / (self.n_clusters - 1) * 90)
            flush_print(f"PROGRESS: {progress}")

        # === 6. Agregasi hasil ===
        self.labels_ = np.full(X.shape[0], -1, dtype=np.int32)
        self.cluster_centers_ = np.empty((self.n_clusters, X.shape[1]), dtype=X.dtype)

        for i, node in enumerate(self._bisecting_tree.iter_leaves()):
            self.labels_[node.indices] = i
            self.cluster_centers_[i] = node.center
            node.label = i
            node.indices = None

        # === 7. Kembalikan mean & hitung inertia ===
        if not sp.issparse(X):
            X += self._X_mean
            self.cluster_centers_ += self._X_mean

        _inertia = _inertia_sparse if sp.issparse(X) else _inertia_dense
        self.inertia_ = _inertia(
            X, sample_weight, self.cluster_centers_, self.labels_, self._n_threads
        )
        self._n_features_out = self.cluster_centers_.shape[0]

        flush_print("PROGRESS: 100")
        flush_print(f"STATUS: Clustering selesai (total inertia={self.inertia_:.3f})")
        flush_print("DONE")

        return self
