//! JSON serialization / dump module for Rose CLI.
//!
//! Converts cached entities to JSON for the `print` and `print-all` CLI commands.

use std::collections::HashMap;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use rose_core::cache::{self, Release, Track};
use rose_core::common::uniq;
use rose_core::config::Config;
use rose_core::releases::find_releases_matching_rule;
use rose_core::rule_parser::{Matcher, Pattern};
use rose_core::tracks::find_tracks_matching_rule;

// ---------------------------------------------------------------------------
// Entity → JSON
// ---------------------------------------------------------------------------

pub fn release_to_json(r: &Release) -> Value {
    json!({
        "id": r.id,
        "source_path": r.source_path.canonicalize()
            .unwrap_or_else(|_| r.source_path.clone())
            .to_string_lossy(),
        "cover_image_path": r.cover_image_path.as_ref().map(|p|
            p.canonicalize()
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy()
                .into_owned()
        ),
        "added_at": r.added_at,
        "releasetitle": r.releasetitle,
        "releasetype": r.releasetype,
        "releasedate": r.releasedate.as_ref().map(|d| d.to_string()),
        "originaldate": r.originaldate.as_ref().map(|d| d.to_string()),
        "compositiondate": r.compositiondate.as_ref().map(|d| d.to_string()),
        "catalognumber": r.catalognumber,
        "edition": r.edition,
        "new": r.new,
        "favorite": r.favorite,
        "rating": r.rating,
        "disctotal": r.disctotal,
        "genres": r.genres,
        "parent_genres": r.parent_genres,
        "secondary_genres": r.secondary_genres,
        "parent_secondary_genres": r.parent_secondary_genres,
        "all_genres": uniq([
            r.genres.as_slice(),
            r.parent_genres.as_slice(),
            r.secondary_genres.as_slice(),
            r.parent_secondary_genres.as_slice(),
        ].concat()),
        "descriptors": r.descriptors,
        "labels": r.labels,
        "releaseartists": r.releaseartists.dump(),
    })
}

pub fn track_to_json(t: &Track, with_release_info: bool) -> Value {
    let mut obj = json!({
        "id": t.id,
        "source_path": t.source_path.canonicalize()
            .unwrap_or_else(|_| t.source_path.clone())
            .to_string_lossy(),
        "tracktitle": t.tracktitle,
        "tracknumber": t.tracknumber,
        "tracktotal": t.tracktotal,
        "discnumber": t.discnumber,
        "duration_seconds": t.duration_seconds,
        "trackartists": t.trackartists.dump(),
    });
    if with_release_info {
        let map = obj.as_object_mut().unwrap();
        map.insert("release_id".into(), json!(t.release.id));
        map.insert("added_at".into(), json!(t.release.added_at));
        map.insert("releasetitle".into(), json!(t.release.releasetitle));
        map.insert("releasetype".into(), json!(t.release.releasetype));
        map.insert("disctotal".into(), json!(t.release.disctotal));
        map.insert(
            "releasedate".into(),
            json!(t.release.releasedate.as_ref().map(|d| d.to_string())),
        );
        map.insert(
            "originaldate".into(),
            json!(t.release.originaldate.as_ref().map(|d| d.to_string())),
        );
        map.insert(
            "compositiondate".into(),
            json!(t.release.compositiondate.as_ref().map(|d| d.to_string())),
        );
        map.insert("catalognumber".into(), json!(t.release.catalognumber));
        map.insert("edition".into(), json!(t.release.edition));
        map.insert("new".into(), json!(t.release.new));
        map.insert("favorite".into(), json!(t.release.favorite));
        map.insert("rating".into(), json!(t.release.rating));
        map.insert("genres".into(), json!(t.release.genres));
        map.insert("parent_genres".into(), json!(t.release.parent_genres));
        map.insert("secondary_genres".into(), json!(t.release.secondary_genres));
        map.insert(
            "parent_secondary_genres".into(),
            json!(t.release.parent_secondary_genres),
        );
        map.insert("descriptors".into(), json!(t.release.descriptors));
        map.insert("labels".into(), json!(t.release.labels));
        map.insert("releaseartists".into(), t.release.releaseartists.dump());
    }
    obj
}

// ---------------------------------------------------------------------------
// Dump functions
// ---------------------------------------------------------------------------

pub fn dump_release(c: &Config, release_id: &str) -> Result<String> {
    let release = cache::get_release(c, release_id)?
        .ok_or_else(|| anyhow::anyhow!("Release {release_id} does not exist"))?;
    let tracks = cache::get_tracks_of_release(c, &release)?;
    let mut rj = release_to_json(&release);
    rj.as_object_mut().unwrap().insert(
        "tracks".into(),
        json!(tracks
            .iter()
            .map(|t| track_to_json(t, false))
            .collect::<Vec<_>>()),
    );
    Ok(serde_json::to_string(&rj)?)
}

pub fn dump_all_releases(c: &Config, matcher: Option<&Matcher>) -> Result<String> {
    let releases = if let Some(m) = matcher {
        find_releases_matching_rule(c, m, true)?
    } else {
        cache::list_releases(c, None, true)?
    };
    let release_tracks = cache::get_tracks_of_releases(c, &releases)?;
    let out: Vec<Value> = release_tracks
        .iter()
        .map(|(release, tracks)| {
            let mut rj = release_to_json(release);
            rj.as_object_mut().unwrap().insert(
                "tracks".into(),
                json!(tracks
                    .iter()
                    .map(|t| track_to_json(t, false))
                    .collect::<Vec<_>>()),
            );
            rj
        })
        .collect();
    Ok(serde_json::to_string(&out)?)
}

pub fn dump_track(c: &Config, track_id: &str) -> Result<String> {
    let track = cache::get_track(c, track_id)?
        .ok_or_else(|| anyhow::anyhow!("Track {track_id} does not exist"))?;
    Ok(serde_json::to_string(&track_to_json(&track, true))?)
}

pub fn dump_all_tracks(c: &Config, matcher: Option<&Matcher>) -> Result<String> {
    let tracks = if let Some(m) = matcher {
        find_tracks_matching_rule(c, m)?
    } else {
        cache::list_tracks(c, None)?
    };
    let out: Vec<Value> = tracks.iter().map(|t| track_to_json(t, true)).collect();
    Ok(serde_json::to_string(&out)?)
}

pub fn dump_artist(c: &Config, artist_name: &str) -> Result<String> {
    if !cache::artist_exists(c, artist_name)? {
        bail!("artist {artist_name} does not exist");
    }
    let m = Matcher::from_expandable(
        &["artist"],
        Pattern::new(artist_name, true, false, false, false),
    );
    let artist_releases = find_releases_matching_rule(c, &m, true)?;
    let roles = partition_releases_by_role(artist_name, &artist_releases);
    let roles_json: HashMap<&str, Vec<Value>> = roles
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().map(release_to_json).collect()))
        .collect();
    Ok(serde_json::to_string(&json!({
        "name": artist_name,
        "roles": roles_json,
    }))?)
}

pub fn dump_all_artists(c: &Config) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for name in cache::list_artists(c)? {
        let m =
            Matcher::from_expandable(&["artist"], Pattern::new(&name, true, false, false, false));
        let artist_releases = find_releases_matching_rule(c, &m, true)?;
        let roles = partition_releases_by_role(&name, &artist_releases);
        let roles_json: HashMap<&str, Vec<Value>> = roles
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(release_to_json).collect()))
            .collect();
        out.push(json!({
            "name": name,
            "roles": roles_json,
        }));
    }
    Ok(serde_json::to_string(&out)?)
}

fn partition_releases_by_role<'a>(
    artist: &str,
    releases: &'a [Release],
) -> HashMap<&'static str, Vec<&'a Release>> {
    let mut rval: HashMap<&'static str, Vec<&'a Release>> = HashMap::new();
    rval.insert("main", Vec::new());
    rval.insert("guest", Vec::new());
    rval.insert("remixer", Vec::new());
    rval.insert("producer", Vec::new());
    rval.insert("composer", Vec::new());
    rval.insert("conductor", Vec::new());
    rval.insert("djmixer", Vec::new());
    for release in releases {
        for (role, names) in release.releaseartists.items() {
            if names.iter().any(|a| a.name == artist) {
                rval.get_mut(role).unwrap().push(release);
                break;
            }
        }
    }
    rval
}

pub fn dump_genre(c: &Config, genre_name: &str) -> Result<String> {
    if !cache::genre_exists(c, genre_name)? {
        bail!("Genre {genre_name} does not exist");
    }
    let m = Matcher::from_expandable(
        &["genre"],
        Pattern::new(genre_name, true, false, false, false),
    );
    let genre_releases = find_releases_matching_rule(c, &m, true)?;
    let releases: Vec<Value> = genre_releases.iter().map(release_to_json).collect();
    Ok(serde_json::to_string(&json!({
        "name": genre_name,
        "releases": releases,
    }))?)
}

pub fn dump_all_genres(c: &Config) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for e in cache::list_genres(c)? {
        let m = Matcher::from_expandable(
            &["genre"],
            Pattern::new(&e.genre, true, false, false, false),
        );
        let genre_releases = find_releases_matching_rule(c, &m, true)?;
        let releases: Vec<Value> = genre_releases.iter().map(release_to_json).collect();
        out.push(json!({
            "name": e.genre,
            "only_new_releases": e.only_new_releases,
            "releases": releases,
        }));
    }
    Ok(serde_json::to_string(&out)?)
}

pub fn dump_label(c: &Config, label_name: &str) -> Result<String> {
    if !cache::label_exists(c, label_name)? {
        bail!("label {label_name} does not exist");
    }
    let m = Matcher::from_expandable(
        &["label"],
        Pattern::new(label_name, true, false, false, false),
    );
    let label_releases = find_releases_matching_rule(c, &m, true)?;
    let releases: Vec<Value> = label_releases.iter().map(release_to_json).collect();
    Ok(serde_json::to_string(&json!({
        "name": label_name,
        "releases": releases,
    }))?)
}

pub fn dump_all_labels(c: &Config) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for e in cache::list_labels(c)? {
        let m = Matcher::from_expandable(
            &["label"],
            Pattern::new(&e.label, true, false, false, false),
        );
        let label_releases = find_releases_matching_rule(c, &m, true)?;
        let releases: Vec<Value> = label_releases.iter().map(release_to_json).collect();
        out.push(json!({
            "name": e.label,
            "only_new_releases": e.only_new_releases,
            "releases": releases,
        }));
    }
    Ok(serde_json::to_string(&out)?)
}

pub fn dump_descriptor(c: &Config, descriptor_name: &str) -> Result<String> {
    if !cache::descriptor_exists(c, descriptor_name)? {
        bail!("descriptor {descriptor_name} does not exist");
    }
    let m = Matcher::from_expandable(
        &["descriptor"],
        Pattern::new(descriptor_name, true, false, false, false),
    );
    let descriptor_releases = find_releases_matching_rule(c, &m, true)?;
    let releases: Vec<Value> = descriptor_releases.iter().map(release_to_json).collect();
    Ok(serde_json::to_string(&json!({
        "name": descriptor_name,
        "releases": releases,
    }))?)
}

pub fn dump_all_descriptors(c: &Config) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for e in cache::list_descriptors(c)? {
        let m = Matcher::from_expandable(
            &["descriptor"],
            Pattern::new(&e.descriptor, true, false, false, false),
        );
        let descriptor_releases = find_releases_matching_rule(c, &m, true)?;
        let releases: Vec<Value> = descriptor_releases.iter().map(release_to_json).collect();
        out.push(json!({
            "name": e.descriptor,
            "only_new_releases": e.only_new_releases,
            "releases": releases,
        }));
    }
    Ok(serde_json::to_string(&out)?)
}

pub fn dump_collage(c: &Config, collage_name: &str) -> Result<String> {
    let _collage = cache::get_collage(c, collage_name)?
        .ok_or_else(|| anyhow::anyhow!("Collage {collage_name} does not exist"))?;
    let collage_releases = cache::get_collage_releases(c, collage_name)?;
    let releases: Vec<Value> = collage_releases
        .iter()
        .enumerate()
        .map(|(idx, rls)| {
            let mut rj = release_to_json(rls);
            rj.as_object_mut()
                .unwrap()
                .insert("position".into(), json!(idx + 1));
            rj
        })
        .collect();
    Ok(serde_json::to_string(&json!({
        "name": collage_name,
        "releases": releases,
    }))?)
}

pub fn dump_all_collages(c: &Config) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for name in cache::list_collages(c)? {
        let _collage = cache::get_collage(c, &name)?;
        let collage_releases = cache::get_collage_releases(c, &name)?;
        let releases: Vec<Value> = collage_releases
            .iter()
            .enumerate()
            .map(|(idx, rls)| {
                let mut rj = release_to_json(rls);
                rj.as_object_mut()
                    .unwrap()
                    .insert("position".into(), json!(idx + 1));
                rj
            })
            .collect();
        out.push(json!({
            "name": name,
            "releases": releases,
        }));
    }
    Ok(serde_json::to_string(&out)?)
}

pub fn dump_playlist(c: &Config, playlist_name: &str) -> Result<String> {
    let playlist = cache::get_playlist(c, playlist_name)?
        .ok_or_else(|| anyhow::anyhow!("Playlist {playlist_name} does not exist"))?;
    let playlist_tracks = cache::get_playlist_tracks(c, playlist_name)?;
    let tracks: Vec<Value> = playlist_tracks
        .iter()
        .enumerate()
        .map(|(idx, trk)| {
            let mut tj = track_to_json(trk, true);
            tj.as_object_mut()
                .unwrap()
                .insert("position".into(), json!(idx + 1));
            tj
        })
        .collect();
    Ok(serde_json::to_string(&json!({
        "name": playlist_name,
        "cover_image_path": playlist.cover_path.map(|p| p.to_string_lossy().into_owned()),
        "tracks": tracks,
    }))?)
}

pub fn dump_all_playlists(c: &Config) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for name in cache::list_playlists(c)? {
        let playlist = cache::get_playlist(c, &name)?;
        let playlist_tracks = cache::get_playlist_tracks(c, &name)?;
        let tracks: Vec<Value> = playlist_tracks
            .iter()
            .enumerate()
            .map(|(idx, trk)| {
                let mut tj = track_to_json(trk, true);
                tj.as_object_mut()
                    .unwrap()
                    .insert("position".into(), json!(idx + 1));
                tj
            })
            .collect();
        out.push(json!({
            "name": name,
            "cover_image_path": playlist.and_then(|p| p.cover_path.map(|cp| cp.to_string_lossy().into_owned())),
            "tracks": tracks,
        }));
    }
    Ok(serde_json::to_string(&out)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    use rose_core::cache::{connect, maybe_invalidate_cache_database};

    /// Create a minimal config pointing at a temporary directory.
    fn test_config(dir: &TempDir) -> Config {
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cfg_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
            music_source_dir = "{}"
            cache_dir = "{}"
            vfs.mount_dir = "{}"
            "#,
            music_dir.display(),
            cache_dir.display(),
            dir.path().join("vfs").display(),
        )
        .unwrap();
        Config::parse(Some(&cfg_path)).unwrap()
    }

    /// Returns a Config whose cache database is fully populated with test data.
    fn seeded_config() -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let music_dir = config.music_source_dir.clone();

        // Create directories and files so source_path references are valid.
        let dirpaths = [
            music_dir.join("r1"),
            music_dir.join("r2"),
            music_dir.join("r3"),
            music_dir.join("r4"),
        ];
        for d in &dirpaths {
            std::fs::create_dir_all(d).unwrap();
        }
        let musicpaths = [
            music_dir.join("r1/01.m4a"),
            music_dir.join("r1/02.m4a"),
            music_dir.join("r2/01.m4a"),
            music_dir.join("r3/01.m4a"),
            music_dir.join("r4/01.m4a"),
        ];
        for p in &musicpaths {
            std::fs::File::create(p).unwrap();
        }
        let imagepaths = [music_dir.join("r2/cover.jpg")];
        for p in &imagepaths {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::File::create(p).unwrap();
        }
        // Create playlist cover image dir + file.
        let playlist_dir = music_dir.join("!playlists");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let playlist_cover = playlist_dir.join("Lala Lisa.jpg");
        std::fs::File::create(&playlist_cover).unwrap();

        let conn = connect(&config).unwrap();
        conn.execute_batch(&format!(
            r#"
INSERT INTO releases
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, title      , releasetype , releasedate , originaldate, compositiondate, catalognumber, edition , disctotal, new  , favorite, metahash)
VALUES ('r1', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 1', 'album'     , '2023'      , null        , null           , null         , null    , 1        , false, true    , '1')
     , ('r2', '{}'           , '{}'             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 2', 'album'     , '2021'      , '2019'      , null           , 'DG-001'     , 'Deluxe', 1        , true , false   , '2')
     , ('r3', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 3', 'album'     , '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '3')
     , ('r4', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 4', 'loosetrack', '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '4');

INSERT INTO releases_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Techno'          , 1)
     , ('r1'      , 'Deep House'      , 2)
     , ('r2'      , 'Modern Classical', 1);

INSERT INTO releases_secondary_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Rominimal'       , 1)
     , ('r1'      , 'Ambient'         , 2)
     , ('r2'      , 'Orchestral Music', 1);

INSERT INTO releases_descriptors
       (release_id, descriptor, position)
VALUES ('r1'      , 'Warm'    , 1)
     , ('r1'      , 'Hot'     , 2)
     , ('r2'      , 'Wet'     , 1);

INSERT INTO releases_labels
       (release_id, label         , position)
VALUES ('r1'      , 'Silk Music'  , 1)
     , ('r2'      , 'Native State', 1);

INSERT INTO tracks
       (id  , source_path    , source_mtime, title    , release_id, tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('t1', '{}'           , '999'       , 'Track 1', 'r1'      , '01'       , 2         , '01'      , 120             , '1')
     , ('t2', '{}'           , '999'       , 'Track 2', 'r1'      , '02'       , 2         , '01'      , 240             , '2')
     , ('t3', '{}'           , '999'       , 'Track 1', 'r2'      , '01'       , 1         , '01'      , 120             , '3')
     , ('t4', '{}'           , '999'       , 'Track 1', 'r3'      , '01'       , 1         , '01'      , 120             , '4')
     , ('t5', '{}'           , '999'       , 'Track 1', 'r4'      , '01'       , 1         , '01'      , 120             , '5');

INSERT INTO releases_artists
       (release_id, artist           , role   , position)
VALUES ('r1'      , 'Techno Man'     , 'main' , 1)
     , ('r1'      , 'Bass Man'       , 'main' , 2)
     , ('r2'      , 'Violin Woman'   , 'main' , 1)
     , ('r2'      , 'Conductor Woman', 'guest', 2);

INSERT INTO tracks_artists
       (track_id, artist           , role   , position)
VALUES ('t1'    , 'Techno Man'     , 'main' , 1)
     , ('t1'    , 'Bass Man'       , 'main' , 2)
     , ('t2'    , 'Techno Man'     , 'main' , 1)
     , ('t2'    , 'Bass Man'       , 'main' , 2)
     , ('t3'    , 'Violin Woman'   , 'main' , 1)
     , ('t3'    , 'Conductor Woman', 'guest', 2);

INSERT INTO collages
       (name       , source_mtime)
VALUES ('Rose Gold', '999')
     , ('Ruby Red' , '999');

INSERT INTO collages_releases
       (collage_name, release_id, position, missing)
VALUES ('Rose Gold' , 'r1'      , 1       , false)
     , ('Rose Gold' , 'r2'      , 2       , false);

INSERT INTO playlists
       (name           , source_mtime, cover_path)
VALUES ('Lala Lisa'    , '999'       , '{}')
     , ('Turtle Rabbit', '999'       , null);

INSERT INTO playlists_tracks
       (playlist_name, track_id, position, missing)
VALUES ('Lala Lisa'  , 't1'    , 1       , false)
     , ('Lala Lisa'  , 't3'    , 2       , false);
            "#,
            dirpaths[0].display(),
            dirpaths[1].display(), imagepaths[0].display(),
            dirpaths[2].display(),
            dirpaths[3].display(),
            musicpaths[0].display(),
            musicpaths[1].display(),
            musicpaths[2].display(),
            musicpaths[3].display(),
            musicpaths[4].display(),
            playlist_cover.display(),
        ))
        .expect("Failed to seed cache database");

        (dir, config)
    }

    // -----------------------------------------------------------------------
    // Release dumps
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_release() {
        let (_dir, config) = seeded_config();
        let json_str = dump_release(&config, "r1").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        // Top-level release keys.
        assert_eq!(obj["id"], "r1");
        assert!(obj.contains_key("source_path"));
        assert_eq!(obj["releasetitle"], "Release 1");
        assert_eq!(obj["releasetype"], "album");
        assert_eq!(obj["releasedate"], "2023");
        assert_eq!(obj["genres"], json!(["Techno", "Deep House"]));
        assert_eq!(obj["labels"], json!(["Silk Music"]));
        assert!(obj["favorite"].as_bool().unwrap());
        assert!(!obj["new"].as_bool().unwrap());

        // Release artists present.
        let artists = obj["releaseartists"].as_object().unwrap();
        assert!(artists.contains_key("main"));

        // Tracks array embedded.
        let tracks = obj["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0]["tracktitle"], "Track 1");
        assert_eq!(tracks[1]["tracktitle"], "Track 2");
    }

    #[test]
    fn test_dump_all_releases() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_releases(&config, None).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // 4 releases seeded (r1, r2, r3, r4).
        assert_eq!(arr.len(), 4);
        // Each entry should have tracks.
        for entry in arr {
            assert!(entry.as_object().unwrap().contains_key("tracks"));
        }
    }

    #[test]
    fn test_dump_releases_with_matcher() {
        let (_dir, config) = seeded_config();
        // Match releases by artist "Violin Woman" (strict lookup, optimized path).
        let m = Matcher::from_expandable(
            &["artist"],
            Pattern::new("Violin Woman", true, false, false, false),
        );
        let json_str = dump_all_releases(&config, Some(&m)).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["releasetitle"], "Release 2");
    }

    // -----------------------------------------------------------------------
    // Track dumps
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_track() {
        let (_dir, config) = seeded_config();
        let json_str = dump_track(&config, "t1").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["id"], "t1");
        assert!(obj.contains_key("source_path"));
        assert_eq!(obj["tracktitle"], "Track 1");
        assert_eq!(obj["tracknumber"], "01");
        assert_eq!(obj["tracktotal"], 2);
        assert_eq!(obj["discnumber"], "01");
        assert_eq!(obj["duration_seconds"], 120);

        // Release info should be present (with_release_info = true).
        assert_eq!(obj["release_id"], "r1");
        assert_eq!(obj["releasetitle"], "Release 1");
        assert_eq!(obj["releasetype"], "album");
        assert!(obj.contains_key("releaseartists"));
        assert!(obj.contains_key("genres"));
        assert!(obj.contains_key("labels"));
    }

    #[test]
    fn test_dump_all_tracks() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_tracks(&config, None).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // 5 tracks seeded (t1-t5).
        assert_eq!(arr.len(), 5);
        for entry in arr {
            let obj = entry.as_object().unwrap();
            assert!(obj.contains_key("id"));
            assert!(obj.contains_key("tracktitle"));
            // Each track should carry release info.
            assert!(obj.contains_key("release_id"));
        }
    }

    // -----------------------------------------------------------------------
    // Entity dumps -- Artist
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_artist() {
        let (_dir, config) = seeded_config();
        let json_str = dump_artist(&config, "Techno Man").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["name"], "Techno Man");
        let roles = obj["roles"].as_object().unwrap();
        // Techno Man is a main artist on r1.
        let main = roles["main"].as_array().unwrap();
        assert_eq!(main.len(), 1);
        assert_eq!(main[0]["id"], "r1");
        // Other role buckets should exist (possibly empty).
        assert!(roles.contains_key("guest"));
        assert!(roles.contains_key("remixer"));
        assert!(roles.contains_key("producer"));
        assert!(roles.contains_key("composer"));
        assert!(roles.contains_key("conductor"));
        assert!(roles.contains_key("djmixer"));
    }

    #[test]
    fn test_dump_all_artists() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_artists(&config).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // Seeded artists: Techno Man, Bass Man, Violin Woman, Conductor Woman.
        assert_eq!(arr.len(), 4);
        let names: Vec<&str> = arr.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Techno Man"));
        assert!(names.contains(&"Bass Man"));
        assert!(names.contains(&"Violin Woman"));
        assert!(names.contains(&"Conductor Woman"));
    }

    // -----------------------------------------------------------------------
    // Entity dumps -- Genre
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_genre() {
        let (_dir, config) = seeded_config();
        let json_str = dump_genre(&config, "Techno").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["name"], "Techno");
        let releases = obj["releases"].as_array().unwrap();
        // r1 has genre "Techno".
        assert!(!releases.is_empty());
        assert!(releases.iter().any(|r| r["id"] == "r1"));
    }

    #[test]
    fn test_dump_all_genres() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_genres(&config).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // Seeded genres: Techno, Deep House, Modern Classical (primaries only
        // in the genres table). Count may vary due to parent genre expansion,
        // but there should be at least 3.
        assert!(arr.len() >= 3);
        for entry in arr {
            let obj = entry.as_object().unwrap();
            assert!(obj.contains_key("name"));
            assert!(obj.contains_key("releases"));
        }
    }

    // -----------------------------------------------------------------------
    // Entity dumps -- Label
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_label() {
        let (_dir, config) = seeded_config();
        let json_str = dump_label(&config, "Silk Music").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["name"], "Silk Music");
        let releases = obj["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0]["id"], "r1");
    }

    #[test]
    fn test_dump_all_labels() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_labels(&config).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // Seeded labels: Silk Music, Native State.
        assert_eq!(arr.len(), 2);
        let names: Vec<&str> = arr.iter().map(|l| l["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Silk Music"));
        assert!(names.contains(&"Native State"));
    }

    // -----------------------------------------------------------------------
    // Entity dumps -- Descriptor
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_descriptor() {
        let (_dir, config) = seeded_config();
        let json_str = dump_descriptor(&config, "Warm").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["name"], "Warm");
        let releases = obj["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0]["id"], "r1");
    }

    #[test]
    fn test_dump_all_descriptors() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_descriptors(&config).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // Seeded descriptors: Warm, Hot, Wet.
        assert_eq!(arr.len(), 3);
        let names: Vec<&str> = arr.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Warm"));
        assert!(names.contains(&"Hot"));
        assert!(names.contains(&"Wet"));
    }

    // -----------------------------------------------------------------------
    // Collection dumps -- Collage
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_collage() {
        let (_dir, config) = seeded_config();
        let json_str = dump_collage(&config, "Rose Gold").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["name"], "Rose Gold");
        let releases = obj["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 2);
        // Positions should be 1-indexed.
        assert_eq!(releases[0]["position"], 1);
        assert_eq!(releases[1]["position"], 2);
        // Release data present.
        assert_eq!(releases[0]["id"], "r1");
        assert_eq!(releases[1]["id"], "r2");
    }

    #[test]
    fn test_dump_all_collages() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_collages(&config).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // Seeded collages: Rose Gold, Ruby Red.
        assert_eq!(arr.len(), 2);
        let names: Vec<&str> = arr.iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Rose Gold"));
        assert!(names.contains(&"Ruby Red"));
    }

    // -----------------------------------------------------------------------
    // Collection dumps -- Playlist
    // -----------------------------------------------------------------------

    #[test]
    fn test_dump_playlist() {
        let (_dir, config) = seeded_config();
        let json_str = dump_playlist(&config, "Lala Lisa").unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let obj = v.as_object().unwrap();

        assert_eq!(obj["name"], "Lala Lisa");
        // cover_image_path should be non-null for Lala Lisa.
        assert!(obj["cover_image_path"].is_string());
        let tracks = obj["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        // Positions should be 1-indexed.
        assert_eq!(tracks[0]["position"], 1);
        assert_eq!(tracks[1]["position"], 2);
        // Track data present with release info.
        assert_eq!(tracks[0]["id"], "t1");
        assert!(tracks[0].as_object().unwrap().contains_key("release_id"));
    }

    #[test]
    fn test_dump_all_playlists() {
        let (_dir, config) = seeded_config();
        let json_str = dump_all_playlists(&config).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let arr = v.as_array().unwrap();
        // Seeded playlists: Lala Lisa, Turtle Rabbit.
        assert_eq!(arr.len(), 2);
        let names: Vec<&str> = arr.iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Lala Lisa"));
        assert!(names.contains(&"Turtle Rabbit"));
        // Turtle Rabbit has no cover.
        let turtle = arr.iter().find(|p| p["name"] == "Turtle Rabbit").unwrap();
        assert!(turtle["cover_image_path"].is_null());
    }
}
