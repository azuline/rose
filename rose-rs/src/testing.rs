use rusqlite::Connection;
use std::fs;
#[cfg(test)]
use std::sync::Once;
use tempfile::TempDir;

#[cfg(test)]
static INIT: Once = Once::new();

#[cfg(test)]
pub fn init() -> TempDir {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")))
            .with_test_writer()
            .try_init();
    });
    TempDir::new().expect("failed to create temp dir")
}

// Creates a test config with directories but no files or database
#[cfg(test)]
pub fn config() -> (crate::common::Config, TempDir) {
    let temp_dir = init();
    let base_path = temp_dir.path();

    // Create necessary directories
    fs::create_dir_all(base_path.join("cache")).expect("failed to create cache dir");
    fs::create_dir_all(base_path.join("source")).expect("failed to create source dir");
    fs::create_dir_all(base_path.join("mount")).expect("failed to create mount dir");

    let config = crate::common::Config { max_filename_bytes: 180 };
    (config, temp_dir)
}

// Creates a test environment with a seeded cache with fake testdata. The files on disk are not real.
#[cfg(test)]
pub fn seeded_cache() -> (crate::common::Config, TempDir) {
    let (config, temp_dir) = config();
    let base_path = temp_dir.path();
    let source_dir = base_path.join("source");

    let dirpaths = vec![source_dir.join("r1"), source_dir.join("r2"), source_dir.join("r3"), source_dir.join("r4")];
    let musicpaths = [
        source_dir.join("r1").join("01.m4a"),
        source_dir.join("r1").join("02.m4a"),
        source_dir.join("r2").join("01.m4a"),
        source_dir.join("r3").join("01.m4a"),
        source_dir.join("r4").join("01.m4a"),
    ];
    let imagepaths = [source_dir.join("r2").join("cover.jpg"), source_dir.join("!playlists").join("Lala Lisa.jpg")];

    let conn = Connection::open(base_path.join("cache").join("cache.sqlite3")).expect("failed to open database");
    conn.execute_batch(include_str!("cache.sql")).expect("failed to create schema");

    // Insert test data
    let sql = format!(
        r#"
INSERT INTO releases
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, title      , releasetype , releasedate , originaldate, compositiondate, catalognumber, edition , disctotal, new  , metahash)
VALUES ('r1', '{dirpath0}'   , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 1', 'album'     , '2023'      , null        , null           , null         , null    , 1        , false, '1')
     , ('r2', '{dirpath1}'   , '{imagepath0}'   , '0000-01-01T00:00:00+00:00', '999'         , 'Release 2', 'album'     , '2021'      , '2019'      , null           , 'DG-001'     , 'Deluxe', 1        , true , '2')
     , ('r3', '{dirpath2}'   , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 3', 'album'     , '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, '3')
     , ('r4', '{dirpath3}'   , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 4', 'loosetrack', '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, '4');

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
       (id  , source_path      , source_mtime, title    , release_id, tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('t1', '{musicpath0}'  , '999'       , 'Track 1', 'r1'      , '01'       , 2         , '01'      , 120             , '1')
     , ('t2', '{musicpath1}'  , '999'       , 'Track 2', 'r1'      , '02'       , 2         , '01'      , 240             , '2')
     , ('t3', '{musicpath2}'  , '999'       , 'Track 1', 'r2'      , '01'       , 1         , '01'      , 120             , '3')
     , ('t4', '{musicpath3}'  , '999'       , 'Track 1', 'r3'      , '01'       , 1         , '01'      , 120             , '4')
     , ('t5', '{musicpath4}'  , '999'       , 'Track 1', 'r4'      , '01'       , 1         , '01'      , 120             , '5');

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
VALUES ('Lala Lisa'    , '999',        '{imagepath1}')
     , ('Turtle Rabbit', '999',        null);

INSERT INTO playlists_tracks
       (playlist_name, track_id, position, missing)
VALUES ('Lala Lisa'  , 't1'    , 1       , false)
     , ('Lala Lisa'  , 't3'    , 2       , false);
"#,
        dirpath0 = dirpaths[0].display(),
        dirpath1 = dirpaths[1].display(),
        imagepath0 = imagepaths[0].display(),
        dirpath2 = dirpaths[2].display(),
        dirpath3 = dirpaths[3].display(),
        musicpath0 = musicpaths[0].display(),
        musicpath1 = musicpaths[1].display(),
        musicpath2 = musicpaths[2].display(),
        musicpath3 = musicpaths[3].display(),
        musicpath4 = musicpaths[4].display(),
        imagepath1 = imagepaths[1].display()
    );
    conn.execute_batch(&sql).expect("failed to insert test data");

    conn.create_scalar_function(
        "process_string_for_fts",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            // Handle NULL values
            let s: Option<String> = ctx.get(0)?;
            let result = match s {
                Some(s) if !s.is_empty() => {
                    // In order to have performant substring search, we use FTS and hack it such that every character
                    // is a token. We use "¬" as our separator character, hoping that it is not used in any metadata.
                    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join("¬")
                }
                _ => String::new(),
            };
            Ok(result)
        },
    )
    .expect("failed to create process_string_for_fts function");
    conn.execute_batch(
        r#"
        INSERT INTO rules_engine_fts (
            rowid
          , tracktitle
          , tracknumber
          , discnumber
          , releasetitle
          , releasedate
          , originaldate
          , compositiondate
          , catalognumber
          , edition
          , releasetype
          , genre
          , secondarygenre
          , descriptor
          , label
          , releaseartist
          , trackartist
          , new
        )
        SELECT
            t.rowid
          , process_string_for_fts(t.title) AS tracktitle
          , process_string_for_fts(t.tracknumber) AS tracknumber
          , process_string_for_fts(t.discnumber) AS discnumber
          , process_string_for_fts(r.title) AS releasetitle
          , process_string_for_fts(r.releasedate) AS releasedate
          , process_string_for_fts(r.originaldate) AS originaldate
          , process_string_for_fts(r.compositiondate) AS compositiondate
          , process_string_for_fts(r.catalognumber) AS catalognumber
          , process_string_for_fts(r.edition) AS edition
          , process_string_for_fts(r.releasetype) AS releasetype
          , process_string_for_fts(COALESCE(GROUP_CONCAT(rg.genre, ' '), '')) AS genre
          , process_string_for_fts(COALESCE(GROUP_CONCAT(rs.genre, ' '), '')) AS secondarygenre
          , process_string_for_fts(COALESCE(GROUP_CONCAT(rd.descriptor, ' '), '')) AS descriptor
          , process_string_for_fts(COALESCE(GROUP_CONCAT(rl.label, ' '), '')) AS label
          , process_string_for_fts(COALESCE(GROUP_CONCAT(ra.artist, ' '), '')) AS releaseartist
          , process_string_for_fts(COALESCE(GROUP_CONCAT(ta.artist, ' '), '')) AS trackartist
          , process_string_for_fts(CASE WHEN r.new THEN 'true' ELSE 'false' END) AS new
        FROM tracks t
        JOIN releases r ON r.id = t.release_id
        LEFT JOIN releases_genres rg ON rg.release_id = r.id
        LEFT JOIN releases_secondary_genres rs ON rs.release_id = r.id
        LEFT JOIN releases_descriptors rd ON rd.release_id = r.id
        LEFT JOIN releases_labels rl ON rl.release_id = r.id
        LEFT JOIN releases_artists ra ON ra.release_id = r.id
        LEFT JOIN tracks_artists ta ON ta.track_id = t.id
        GROUP BY t.id
    "#,
    )
    .expect("failed to insert FTS data");

    fs::create_dir_all(source_dir.join("!collages")).expect("failed to create !collages");
    fs::create_dir_all(source_dir.join("!playlists")).expect("failed to create !playlists");
    for d in &dirpaths {
        fs::create_dir_all(d).expect("failed to create dir");
        let filename = d.file_name().unwrap().to_str().unwrap();
        fs::write(d.join(format!(".rose.{filename}.toml")), "").expect("failed to create toml");
    }
    for f in musicpaths.iter().chain(imagepaths.iter()) {
        fs::write(f, "").expect("failed to create file");
    }
    for cn in ["Rose Gold", "Ruby Red"] {
        fs::write(source_dir.join("!collages").join(format!("{cn}.toml")), "").expect("failed to create collage toml");
    }
    for pn in ["Lala Lisa", "Turtle Rabbit"] {
        fs::write(source_dir.join("!playlists").join(format!("{pn}.toml")), "").expect("failed to create playlist toml");
    }

    (config, temp_dir)
}
