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
