# Critical Implementation Notes for Rose Rust Port

## ⚠️ Easy-to-Miss Behaviors That Will Cause Bugs

### 1. **Track/Disc Totals Are Calculated, Not Stored**
The Python code calculates track totals by counting tracks per disc during cache updates. These are NOT read from audio tags:
```python
# In cache.py around line 845-856
totals_ctr[track.discnumber] += 1
# Later...
tracktotal = totals_ctr[track.discnumber]
```
**Why it matters:** If you read tracktotal from tags, counts will be wrong for releases with missing tracks.

### 2. **Release Artists Can Be Empty** 
The Python code handles empty artist arrays by returning "Unknown Artists":
```python
# In templates.py line 78-79
if r == "":
    return "Unknown Artists"
```
**Why it matters:** Empty artist handling affects sorting, display, and path generation.

### 3. **In-Progress Directory Detection Is Critical**
The Python code detects directories being written by other tools:
```python
# In cache.py around line 611-621
if release_id_from_first_file and not force:
    logger.warning(f"No-Op: Skipping release at {source_path}...")
    continue
```
**Why it matters:** Without this, concurrent tools (like beets) can corrupt the cache.

### 4. **Datafiles Are Version-Upgraded In Place**
When reading `.rose.{uuid}.toml`, missing fields are added with defaults and written back:
```python
# In cache.py around line 664-669
if new_resolved_data != diskdata:
    with lock(c, lockname), datafile_path.open("wb") as fp:
        tomli_w.dump(new_resolved_data, fp)
```
**Why it matters:** Old datafiles must be upgraded transparently.

### 5. **File Extension Preservation Has Special Rules**
Extensions > 6 characters are considered "bullshit" and not preserved:
```python
# In common.py around line 134-137
if len(ext.encode()) > 6:
    stem = name
    ext = ""
```
**Why it matters:** Files like `song.verylongextension` get truncated differently.

### 6. **Multiprocessing Threshold Is Not Just Optimization**
The < 50 releases threshold exists because virtual filesystem and watchdog use threads:
```python
# In cache.py around line 396-397
# Starting other processes from threads is bad!
if not force_multiprocessing and len(release_dirs) < 50:
```
**Why it matters:** Using multiprocessing from threads can deadlock.

### 7. **Lock Timeouts Are Retries, Not Failures**
The lock system actively waits and retries, not just fails:
```python
# In cache.py around line 166-169
if row and row[0] and row[0] > time.time():
    sleep = max(0, row[0] - time.time())
    time.sleep(sleep)
    continue
```
**Why it matters:** Locks should rarely fail in practice.

### 8. **Cover Art Detection Uses Lowercase Matching**
All cover art operations compare lowercase filenames:
```python
# In cache.py line 677
if f.name.lower() in c.valid_cover_arts:
```
**Why it matters:** `COVER.JPG` must be detected as cover art.

### 9. **Artist Aliases Are Never Written to Tags**
The alias field is for display only and stripped during serialization:
```python
# In releases.py line 188
if not art.alias
```
**Why it matters:** Aliases affect cache and display but not files.

### 10. **Path Templates Use Jinja2 Whitespace Collapsing**
Multiple spaces/newlines collapse to single space:
```python
# In templates.py - _collapse_spacing function
re.sub(r"\s+", " ", s).strip()
```
**Why it matters:** Template output must match exactly for path generation.

## 🔍 Subtle State Management Issues

### 1. **Release Dirty Flag Pattern**
The cache update uses a dirty flag to minimize database writes:
```python
release_dirty = False
# ... various checks that might set it to True
if release_dirty:
    # Schedule database update
```
**Implement carefully:** Missing a dirty flag set causes stale cache data.

### 2. **Track IDs in Two Places**
Track IDs are stored both in files (as tags) and in the database:
- First time: Generate UUID, write to file, write to database
- Subsequent: Read from file, compare with database
**Why it matters:** Mismatch between file and database IDs indicates corruption.

### 3. **Position Counters in Templates**
Collages and playlists pass position to templates, but it's 1-indexed for display:
```python
# Position in template context is for human display
position: str | None  # "1", "2", "3", etc.
```

### 4. **Delete Operations Don't Remove Files**
All deletes use `send2trash` library, moving to trash instead of unlinking:
```python
send2trash(release.source_path)
```
**Why it matters:** Users expect ability to recover from mistakes.

## 🚨 Concurrency Gotchas

### 1. **Cache Updates Must Lock Releases**
But only when writing:
- Reading datafile: No lock
- Writing datafile: Acquire lock
- Reading for cache update: No lock
- Writing IDs to audio files: No lock (first time only)

### 2. **Collage/Playlist Locks Are Per-Name**
Not per-file:
```python
lock(c, collage_lock_name(name))  # Not path-based!
```

### 3. **SQLite Timeout Is 15 Seconds**
```python
timeout=15.0  # In connect()
```
This is much longer than individual lock timeouts (typically 1-2 seconds).

## 📝 Data Format Quirks

### 1. **TOML None Handling**
TOML has no null, so None becomes empty string:
```python
data["edition"] = self.edition or ""
```

### 2. **Date Serialization**
Dates serialize to strings in TOML but need parsing on read:
```python
RoseDate.parse(d["originaldate"])  # Can handle empty string
```

### 3. **Artist Role Case Sensitivity**
Role comparison is case-insensitive:
```python
getattr(m, a.role.lower())  # Note the .lower()
```

## 🎯 Performance Critical Sections

### 1. **Mtime Checking**
This is THE hot path - called for every file:
```python
if cached_track and track_mtime == cached_track.source_mtime and not force:
    continue  # Skip re-read
```

### 2. **Batch SQL Operations**
All SQL updates are batched at the end of cache update:
```python
upd_release_args.append([...])  # Accumulate
# Later: executemany()
```

### 3. **Template Compilation**
Templates are compiled once and cached:
```python
@cached_property
def compiled(self) -> jinja2.Template:
```

## ✅ Test Coverage Gaps to Fill

The Python tests miss some edge cases that should be tested in Rust:

1. Unicode normalization differences between platforms
2. Concurrent datafile upgrades  
3. Lock expiry during operation
4. Template rendering with all None values
5. Files with no extension vs empty extension
6. Symlink handling in source directory
7. Case sensitivity in genre/label names
8. Maximum path length handling
9. Invalid UTF-8 in filenames
10. Clock skew effects on mtime comparison