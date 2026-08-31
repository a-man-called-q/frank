# Phosphor Light subset

This directory contains the small, vendored Phosphor Light icon subset used by
the gallery. The source is the official `@phosphor-icons/core` **2.1.1** package,
which is also the source of truth behind [Phosphor Icons](https://phosphoricons.com/).
The exact asset list, upstream names, source URLs, and SHA-256 hashes are in
[`manifest.toml`](manifest.toml). The source family is licensed under the MIT
license (see `LICENSE`). SVGs use `currentColor` so the gallery can apply
role-based dark/light tinting at render time.

## Updating the subset

1. Fetch the same `@phosphor-icons/core` release from the official npm registry.
2. Copy only the files listed in `manifest.toml` from its `assets/light/`
   directory, renaming them to the local semantic names where documented.
3. Recompute SHA-256 hashes and update the manifest in the same change.
4. Run the gallery asset tests; unlisted files and hash mismatches are errors.
