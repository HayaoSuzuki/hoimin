# Issue 616: preserve selected directories

Selected empty fixture directories must exist in each worker and be restored after a mutant removes them or replaces them with a file/link. Keep regular-file manifest entries and byte accounting intact; add a sorted directory inventory to the existing manifest and derive it through the same ignore/default/literal/glob policy. Directory permissions remain worker-writable; exact directory permission replication is outside the existing copy contract.

Normal walking records accepted directories. The explicit include walk records only directories actually whitelisted by its override matcher; traversal-only directories are not selected. Add ancestor closure for selected files/directories so accepted descendants remain reachable. Excluded and session/default-protected trees remain excluded. Symlinks are not copied or followed.

Create selected directories in the private snapshot and fresh worker. Reset removes stray entries and file/link replacements of expected directories, then recreates directory paths through retained-root no-follow operations before restoring files. Contract checks compare both file and directory inventories. Original integrity checks include selected directory presence. Empty directories add no logical bytes, copy allowance, or workspace-byte charge; filesystem overhead remains outside byte accounting.

A small Lean state model covers selected/excluded directory presence and reset idempotence. Ancestor closure and include traversal are checked by Rust public tests. Public WorkspacePlan/WorkerWorkspace adapters exercise absent/file/link/extra states; platform-specific symlinks are separated from portable fixtures. This proves finite selection/reset observations, not operating-system race safety. Existing retained-root race tests continue to cover that boundary.

## Design self-review

1. Policy review: an include walker yields unmatched directories for traversal, so blindly recording every yielded directory would restore ignored unrelated trees. Require whitelist matching in that pass and reconstruct only selected ancestors. Normal walk still supplies ordinary unignored empty directories.
2. Restoration review: directory paths can be replaced with files or symlinks. Remove conflicts using existing retained-root entry operations and create ancestors through no-follow handles. Do not rely on lexical path create_dir_all in a previously exposed worker.
3. Compatibility review: preserve file-only entries() callers and byte budgets. Add directory equality to original integrity and reset contracts, otherwise empty-directory mutation would escape observation. Directory metadata permissions are deliberately not preserved; no new flags are needed.
