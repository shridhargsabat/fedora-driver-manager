# Regenerating the vendor tarball

The spec builds with vendored crates.io dependencies (Fedora Rust
guidelines, "building with vendored dependencies"). Regenerate whenever
`Cargo.lock` changes:

```bash
cd /home/shridharsabat/tools/drivers
cargo vendor vendor/
tar --sort-name --mtime='2026-01-01' -czf packaging/fedora-driver-manager-0.1.0-vendor.tar.gz vendor
rm -rf vendor/
```

Then rebuild the SRPM (Task 3.4) so the new `Source1` is included.
