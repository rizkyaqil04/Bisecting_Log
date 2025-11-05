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

from progress import flush_print, ProgressManager  # shared progress utilities


class VerboseBisectingKMeans(BisectingKMeans):
    """
    Bisecting KMeans variant with real-time status and unified progress reporting.

    Args:
        Follows sklearn.cluster.BisectingKMeans constructor semantics.

    Returns:
        None
    """

    def fit_verbose(self, X, y=None, sample_weight=None, progress_manager: ProgressManager = None, plot: bool = False):
        """
        Fit the bisecting k-means model while reporting STATUS and unified PROGRESS.

        Args:
            X: Feature matrix (dense or CSR sparse).
            y: Ignored (for API compatibility).
            sample_weight: Optional sample weights.
            progress_manager (ProgressManager): Optional shared progress manager. When provided,
                all PROGRESS prints will be emitted by the shared manager so the whole pipeline
                shows a single unified PROGRESS percentage. If not provided, local progress
                prints are used (backward-compatible).
            plot (bool): If True, show a matplotlib plot after each bisection step.
                If the data has >2 features, a PCA projection to 2D is applied for plotting.

        Returns:
            self
        """
        # Validate input
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

        # Compute BKM internal total units (same formula used elsewhere)
        bkm_total_units = 4 + max(1, self.n_clusters - 1) + 3
        local_current = 1

        def local_report_status_and_progress_local(msg: str):
            """Fallback local reporting (used when no progress_manager provided)."""
            nonlocal local_current
            flush_print(f"STATUS: {msg}")
            pct = int(round(local_current / bkm_total_units * 100))
            flush_print(f"PROGRESS: {pct}")

        def local_advance_local():
            nonlocal local_current
            local_current += 1
            pct = int(round(local_current / bkm_total_units * 100))
            flush_print(f"PROGRESS: {pct}")

        # Initial STATUS / PROGRESS
        if progress_manager is None:
            local_report_status_and_progress_local("Starting BisectingKMeans initialization")
        else:
            # Print a STATUS line (internal step) but let progress_manager handle PROGRESS
            flush_print("STATUS: Starting BisectingKMeans initialization")
            progress_manager.advance(1)

        # Setup algorithm (lloyd/elkan)
        if self.algorithm == "lloyd" or self.n_clusters == 1:
            from sklearn.cluster._kmeans import _kmeans_single_lloyd
            self._kmeans_single = _kmeans_single_lloyd
            self._check_mkl_vcomp(X, X.shape[0])
        else:
            from sklearn.cluster._kmeans import _kmeans_single_elkan
            self._kmeans_single = _kmeans_single_elkan

        if progress_manager is None:
            local_advance_local()
            flush_print("STATUS: KMeans algorithm backend configured")
        else:
            flush_print("STATUS: KMeans algorithm backend configured")
            progress_manager.advance(1)

        # Centering data
        if not sp.issparse(X):
            self._X_mean = X.mean(axis=0)
            X -= self._X_mean
        x_squared_norms = row_norms(X, squared=True)

        if progress_manager is None:
            local_advance_local()
            flush_print("STATUS: Data centering complete")
        else:
            flush_print("STATUS: Data centering complete")
            progress_manager.advance(1)

        # Create root tree
        from sklearn.cluster._bisect_k_means import _BisectingTree
        self._bisecting_tree = _BisectingTree(
            indices=np.arange(X.shape[0]),
            center=X.mean(axis=0),
            score=0,
        )

        if progress_manager is None:
            local_advance_local()
            flush_print("STATUS: Root cluster created")
        else:
            flush_print("STATUS: Root cluster created")
            progress_manager.advance(1)

        # Main bisection iterations
        for step in range(self.n_clusters - 1):
            cluster_to_bisect = self._bisecting_tree.get_cluster_to_bisect()

            flush_print(f"STATUS: Bisecting cluster {step+1} of {max(1, self.n_clusters-1)} (size={len(cluster_to_bisect.indices)})")
            self._bisect(X, x_squared_norms, sample_weight, cluster_to_bisect)

            # If plotting requested, prepare 2D data (PCA if needed) and plot current partition
            if plot:
                try:
                    import matplotlib.pyplot as plt
                    from sklearn.decomposition import PCA
                except Exception as e:
                    flush_print(f"STATUS: Plot skipped (import error: {e})")
                else:
                    # build current leaf assignment
                    n_samples = X.shape[0]
                    current_assign = np.full(n_samples, -1, dtype=int)
                    leaves = list(self._bisecting_tree.iter_leaves())
                    for i_node, node in enumerate(leaves):
                        # node.indices can be numpy array of sample indices
                        if node.indices is not None:
                            current_assign[node.indices] = i_node

                    # Prepare original-space data for plotting:
                    if sp.issparse(X):
                        X_plot = X.toarray()
                        add_mean = False
                    else:
                        # X currently centered; add mean back for plotting so coordinates look familiar
                        X_plot = X + self._X_mean
                        add_mean = True

                    # project to 2D if needed
                    if X_plot.shape[1] > 2:
                        pca = PCA(n_components=2, random_state=self._random_state)
                        X2 = pca.fit_transform(X_plot)
                        # transform centers
                        centers = np.vstack([node.center + (self._X_mean if add_mean else 0) for node in leaves])
                        centers2 = pca.transform(centers)
                    else:
                        X2 = X_plot if X_plot.ndim == 2 else X_plot.reshape(n_samples, -1)
                        centers2 = np.vstack([node.center + (self._X_mean if add_mean else 0) for node in leaves])

                    # simple scatter plot
                    plt.figure(figsize=(6, 5))
                    cmap = plt.get_cmap("tab20")
                    valid_mask = current_assign >= 0
                    if valid_mask.any():
                        sc = plt.scatter(X2[valid_mask, 0], X2[valid_mask, 1], c=current_assign[valid_mask],
                                         cmap=cmap, s=30, alpha=0.6, edgecolors="none")
                    else:
                        plt.scatter(X2[:, 0], X2[:, 1], s=30, alpha=0.6)

                    # plot centers
                    plt.scatter(centers2[:, 0], centers2[:, 1], marker="x", c="k", s=120, linewidths=2)
                    plt.title(f"Bisect step {step+1}/{max(1, self.n_clusters-1)} - {len(leaves)} clusters")
                    plt.xlabel("PC1" if X2.shape[1] == 2 and X_plot.shape[1] > 2 else "Feature 1")
                    plt.ylabel("PC2" if X2.shape[1] == 2 and X_plot.shape[1] > 2 else "Feature 2")
                    plt.tight_layout()
                    plt.show()

            if progress_manager is None:
                local_advance_local()
            else:
                progress_manager.advance(1)

        # Aggregating results into labels and centers
        self.labels_ = np.full(X.shape[0], -1, dtype=np.int32)
        self.cluster_centers_ = np.empty((self.n_clusters, X.shape[1]), dtype=X.dtype)

        for i, node in enumerate(self._bisecting_tree.iter_leaves()):
            self.labels_[node.indices] = i
            self.cluster_centers_[i] = node.center
            node.label = i
            node.indices = None

        if progress_manager is None:
            local_advance_local()
            flush_print("STATUS: Cluster labels and centers aggregated")
        else:
            flush_print("STATUS: Cluster labels and centers aggregated")
            progress_manager.advance(1)

        # Restore mean (if applied) and compute inertia
        if not sp.issparse(X):
            X += self._X_mean
            self.cluster_centers_ += self._X_mean

        _inertia = _inertia_sparse if sp.issparse(X) else _inertia_dense
        self.inertia_ = _inertia(
            X, sample_weight, self.cluster_centers_, self.labels_, self._n_threads
        )
        self._n_features_out = self.cluster_centers_.shape[0]

        if progress_manager is None:
            local_advance_local()
            pct = int(round(local_current / bkm_total_units * 100))
            flush_print(f"PROGRESS: {pct}")
            flush_print(f"STATUS: Clustering finished (total inertia={self.inertia_:.3f})")
            # finalize local
            local_current = bkm_total_units
            pct = int(round(local_current / bkm_total_units * 100))
            flush_print(f"PROGRESS: {pct}")
            flush_print("DONE")
        else:
            progress_manager.advance(1)
            flush_print(f"STATUS: Clustering finished (total inertia={self.inertia_:.3f})")
            # ensure internal BKM units consumed - progress_manager already advanced per unit above

        return self