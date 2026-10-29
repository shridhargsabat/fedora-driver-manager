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
   VERIFY the new version actually reached repo metadata afterwards:
   `dnf5 repoquery --available <name>.x86_64` must list it. COPR's
   composed metadata can lag or stall (seen during Pulp migration):
   if missing, run `copr-cli regenerate-repos <owner>/<project>`,
   wait, re-check; do NOT announce until the version resolves.
7. Attach RPMs to the GitHub release so visitors can download directly:
   `copr-cli download-build -r fedora-44-x86_64 --rpms --dest <dir> <BUILDID>`,
   then `gh release upload vX.Y.Z <dir>/fedora-44-x86_64/fedora-driver-manager-*.fc44.x86_64.rpm`
   (plus the `.src.rpm`). Verify with `gh release view vX.Y.Z --json assets`.

## Updater notes

- No release-step changes: update detection is computed live from dnf/fwupd.
- The auto-check timer only runs while the app runs; there is deliberately
  no systemd background service in this iteration.
