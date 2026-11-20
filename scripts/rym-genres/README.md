RYM Genre list sourced from https://github.com/FlakyBlueJay/mb-rym-hierarchy (`RateYourMusic
Hierarchy.txt`, copied verbatim to `rym.txt`). The script here transforms its `Genres` and `Scenes &
Movements` sections into a Python data structure:

```bash
MUSIC_ROOT="$(git rev-parse --show-toplevel)" scripts/rym-genres/generate.py
```
