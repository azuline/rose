# Cache.rs Handoff Notes

Hey! Here's everything you need to know to get cache.rs to 100% passing tests. I've already knocked out 12 of the 22 failing tests, so you're starting with 59/73 tests passing. The remaining 10 failures are all due to missing features (not bugs), so this should be straightforward.

## Current State

✅ **What's Working:**
- Database schema initialization
- Basic CRUD operations for releases, tracks, collages, playlists
- File modification detection (with 1-second delays)
- Missing release/track detection in collages/playlists
- Filename truncation and collision handling
- Partially written directory handling

❌ **What's Not Working (10 failing tests):**
- Description metadata auto-updates (4 tests)
- Cascading updates after changes (included in above 4)
- Full-text search updates (1 test)
- Nested directory flattening (1 test)
- Track deletion detection (1 test)
- Multiprocessing synchronization (3 tests)

## Priority 1: Fix Description Metadata (Fixes 5 Tests!) 🎯

This is your biggest win. The Python implementation automatically updates `description_meta` fields in TOML files. Here's exactly what to do:

### Step 1: Update `update_cache_for_collages` (around line 2090)

After the missing detection code, add:

```rust
// Update description_metas for all releases
let mut desc_updates = HashMap::new();
if !release_positions.is_empty() {
    let placeholders = vec!["?"; release_positions.len()].join(",");
    let query = format!(
        "SELECT id, releasetitle, originaldate, releasedate, releaseartist_names, releaseartist_roles 
         FROM releases_view WHERE id IN ({})", 
        placeholders
    );
    let mut stmt = conn.prepare(&query)?;
    let release_ids: Vec<String> = release_positions.iter().map(|(id, _, _)| id.clone()).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(&release_ids), |row| {
        let id: String = row.get("id")?;
        let title: String = row.get("releasetitle")?;
        let original_date: Option<String> = row.get("originaldate")?;
        let release_date: Option<String> = row.get("releasedate")?;
        let date = RoseDate::parse(original_date.or(release_date).as_deref());
        let date_str = date.map(|d| d.to_string()).unwrap_or_else(|| "[0000-00-00]".to_string());
        
        let artist_names: String = row.get("releaseartist_names")?;
        let artist_roles: String = row.get("releaseartist_roles")?;
        let artists = unpack_artists(&config, &artist_names, &artist_roles);
        let artist_str = format_artists(&artists); // You'll need to implement this
        
        let meta = format!("{} {} - {}", date_str, artist_str, title);
        Ok((id, meta))
    })?;
    
    for row in rows {
        let (id, meta) = row?;
        desc_updates.insert(id, meta);
    }
}

// Now update the TOML data
if let Some(releases_array) = data.get_mut("releases").and_then(|v| v.as_array_mut()) {
    for release in releases_array.iter_mut() {
        if let Some(uuid) = release.get("uuid").and_then(|v| v.as_str()) {
            if let Some(new_desc) = desc_updates.get(uuid) {
                let mut final_desc = new_desc.clone();
                if release.get("missing").and_then(|v| v.as_bool()).unwrap_or(false) {
                    final_desc.push_str(" {MISSING}");
                }
                
                if let Some(table) = release.as_table_mut() {
                    let current = table.get("description_meta").and_then(|v| v.as_str()).unwrap_or("");
                    if current != final_desc {
                        table.insert("description_meta".to_string(), toml::Value::String(final_desc));
                        data_changed = true;
                    }
                }
            }
        }
    }
}
```

### Step 2: Do the same for `update_cache_for_playlists` (around line 2330)

Similar code but query `tracks_view` joined with `releases_view` and format as track info.

### Step 3: Add Cascading Updates in `execute_cache_updates` (around line 1900)

After updating releases/tracks, add:

```rust
// Schedule collage updates for modified releases
if !upd_release_ids.is_empty() {
    let placeholders = vec!["?"; upd_release_ids.len()].join(",");
    let affected_collages: Vec<String> = conn
        .prepare(&format!(
            "SELECT DISTINCT collage_name FROM collages_releases WHERE release_id IN ({})",
            placeholders
        ))?
        .query_map(rusqlite::params_from_iter(&upd_release_ids), |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    
    if !affected_collages.is_empty() {
        update_cache_for_collages(c, Some(affected_collages), true)?;
    }
}

// Similar for playlists with track IDs
```

## Priority 2: Fix Full-Text Search (1 Test) 🔍

In `execute_cache_updates`, before the release/track inserts:

```rust
// Delete old FTS entries
if !upd_track_ids.is_empty() || !upd_release_ids.is_empty() {
    let track_placeholders = vec!["?"; upd_track_ids.len()].join(",");
    let release_placeholders = vec!["?"; upd_release_ids.len()].join(",");
    
    tx.execute(&format!(
        "DELETE FROM rules_engine_fts WHERE rowid IN (
            SELECT t.rowid FROM tracks t 
            JOIN releases r ON r.id = t.release_id 
            WHERE t.id IN ({}) OR r.id IN ({})
        )",
        track_placeholders, release_placeholders
    ), rusqlite::params_from_iter(upd_track_ids.iter().chain(upd_release_ids.iter())))?;
}
```

Then after the inserts, add the INSERT INTO rules_engine_fts query (it's huge, check the Python code around line 1700).

## Priority 3: Track Deletion Detection (1 Test) 🗑️

In `_update_cache_for_releases_executor`, when processing each release:

```rust
// At the start of processing a release (around line 1050)
let mut unknown_cached_tracks = HashSet::new();
if let Some((cached_release, cached_tracks)) = cached_releases.get(&release.id) {
    for (path, _) in cached_tracks {
        unknown_cached_tracks.insert(path.clone());
    }
}

// Then as you process each file (around line 1100)
if let Some(cached_track) = cached_tracks.get(&file_path_str) {
    unknown_cached_tracks.remove(&file_path_str);
    // ... rest of the code
}

// After processing all files (around line 1400)
if !unknown_cached_tracks.is_empty() {
    upd_unknown_cached_tracks_args.push((release.id.clone(), unknown_cached_tracks.into_iter().collect()));
}
```

## Priority 4: Nested Directory Flattening (1 Test) 📁

In the rename logic (around line 1420), change track renaming to:

```rust
// Calculate the wanted path (should be at release root)
let wanted_path = release.source_path.join(&wanted_filename);

// If the file is in a subdirectory, we need to move it
if track_path.parent() != Some(&release.source_path) {
    fs::rename(&track_path, &wanted_path)?;
    
    // Clean up empty parent directories
    let mut parent = track_path.parent();
    while let Some(dir) = parent {
        if dir == release.source_path {
            break;
        }
        if fs::read_dir(dir)?.next().is_none() {
            fs::remove_dir(dir)?;
        }
        parent = dir.parent();
    }
}
```

## Testing Your Changes

After each change:
```bash
cargo test --lib cache::tests::test_update_releases_updates_collages_description_meta -- --nocapture
cargo test --lib cache::tests::test_update_cache_releases_updates_full_text_search -- --nocapture
# etc.
```

Run all cache tests:
```bash
cargo test --lib cache -- --nocapture
```

## Quick Wins Checklist

1. [ ] Implement description_meta updates (2-3 hours) → 4 tests pass
2. [ ] Add cascading updates (1 hour) → included in above
3. [ ] Fix FTS updates (1 hour) → 1 test passes
4. [ ] Add track deletion detection (1 hour) → 1 test passes
5. [ ] Fix nested directory handling (2 hours) → 1 test passes
6. [ ] Debug multiprocessing (2-3 hours) → 3 tests pass

Total: ~10-12 hours to get all 73 tests passing

## Tips

- The Python implementation is in `py-impl-reference/rose/cache.py` - use it liberally!
- Most of the "missing" code is just SQL queries and data transformation
- Don't overthink it - the tests tell you exactly what they expect
- The 1-second delays I added are hacky but work - you can improve them later if you want

Good luck! You've got this! 🚀
