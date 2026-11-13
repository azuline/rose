RYM Genre list sourced from https://github.com/FlakyBlueJay/mb-rym-hierarchy (`RateYourMusic
Hierarchy.txt`, copied verbatim to `rym.txt`). The script here transforms its `Genres` section into
a Python data structure:

```bash
ROSE_ROOT="$(git rev-parse --show-toplevel)" scripts/rym-genres/generate.py
```
