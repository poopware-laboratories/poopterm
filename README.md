# Package indexes

The lists `pkg` reads by default, one per platform: `ios-index` now, and
the Android port's `android-aarch64-index` and `android-x86_64-index`
later. Each line names a package, its version, the SHA-256 and size of
its archive, the archive's URL (in the immutable release it was uploaded
to), and a summary. `ios/packages/release.sh` on `main` writes them; see
`ios/packages/README.md` there.
