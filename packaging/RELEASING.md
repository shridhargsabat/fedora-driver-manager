# Cutting a release (fedora-driver-manager)

1. Add a `## [x.y.z] - YYYY-MM-DD` section on top of `CHANGELOG.md`
   (the `embedded_changelog_parses_with_current_version_present` test
   fails until you do).
2. Bump `version` in `Cargo.toml`, add a `<release>` entry (with
   `<description>`) in `data/*.metainfo.xml`, bump `Version:` in the spec.
3. `cargo build --offline`, `cargo test --offline` (all pass),
   `cargo clippy --all-targets --all-features -- -D warnings` (exit 0),
   `desktop-file-validate` (silent), `appstreamcli validate` (success).
4. Commit `release: bump to x.y.z`, push, tag `vX.Y.Z`, push tag,
   `gh release create`.
5. Vendor tarball: regenerate ONLY if `Cargo.lock` changed since the
   last regen (`git log --oneline -- Cargo.lock`); else copy to the new
   `-x.y.z-` name. Verify gtk4+libadwaita present.
6. Fresh SRPM (`rpmbuild -bs`), `copr-cli build` (f44 + rawhide),
   watch to green, `dnf upgrade`, `rpm -V` clean.
