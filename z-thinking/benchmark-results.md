# Benchmark Results: Python vs Rust

## Status

**Placeholder** — This benchmark needs to be run against a real music library for
meaningful results. The testdata/ directory contains only 3 releases with 6 tracks,
which is too small for statistically significant benchmarks.

## Environment

- Hardware: (to be filled)
- Date: (to be filled)
- Testdata: N releases, M tracks (recommend 50-500 releases for meaningful data)
- Tool: `hyperfine` recommended, or shell `time` with 5+ iterations

## Planned Benchmarks

| Operation | Python (mean +/- std) | Rust (mean +/- std) | Speedup |
|-----------|----------------------|---------------------|---------|
| Cache update (cold) | TBD | TBD | TBD |
| Cache update (warm) | TBD | TBD | TBD |
| List all releases (JSON) | TBD | TBD | TBD |
| List all tracks (JSON) | TBD | TBD | TBD |
| Binary startup (version) | TBD | TBD | TBD |
| VFS readdir latency | TBD | TBD | TBD |

## How to Run

Once both Python and Rust implementations are available, run:

```bash
# Build Rust binary
cargo build --release -p rose-cli --manifest-path rose-rs/Cargo.toml

# Cold cache update benchmark (example with hyperfine)
hyperfine --prepare 'rm -f /tmp/rose-bench/cache.db' \
  'rose cache update' \
  'rose-rs/target/release/rose cache update'

# Warm cache update benchmark
hyperfine 'rose cache update' 'rose-rs/target/release/rose cache update'

# List releases
hyperfine 'rose releases print-all' 'rose-rs/target/release/rose releases print-all'
```

## Notes

- The Python implementation must still be functional when this benchmark is run
  (run BEFORE task 051 deletes Python code).
- For VFS benchmarks, FUSE must be available (may not work in CI).
- Consider generating a synthetic larger library by copying testdata releases N times
  with unique UUIDs for scaling benchmarks.
