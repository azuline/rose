import tempfile
from pathlib import Path

import click
import pytest

from rose.config import (
    Config,
    ConfigNotFoundError,
    InvalidConfigValueError,
    MissingConfigKeyError,
    VirtualFSConfig,
)
from rose.rule_parser import (
    Action,
    Matcher,
    Pattern,
    ReplaceAction,
    Rule,
    SplitAction,
)
from rose.templates import PathTemplate, PathTemplateConfig, PathTemplateTriad


def test_config_minimal() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """
                music_source_dir = "~/.music-src"
                vfs.mount_dir = "~/music"
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.music_source_dir == Path.home() / ".music-src"
        assert c.vfs.mount_dir == Path.home() / "music"


def test_config_full() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        cache_dir = Path(tmpdir) / "cache"
        with path.open("w") as fp:
            fp.write(
                f"""
                music_source_dir = "~/.music-src"
                cache_dir = "{cache_dir}"
                max_proc = 8
                artist_aliases = [
                  {{ artist = "Abakus", aliases = ["Cinnamon Chasers"] }},
                  {{ artist = "tripleS", aliases = ["EVOLution", "LOVElution", "+(KR)ystal Eyes", "Acid Angel From Asia", "Acid Eyes"] }},
                ]

                cover_art_stems = [ "aa", "bb" ]
                valid_art_exts = [ "tiff" ]
                write_parent_genres = true
                max_filename_bytes = 255
                ignore_release_directories = [ "dummy boy" ]
                rename_source_files = true

                [[stored_metadata_rules]]
                matcher = "tracktitle:lala"
                actions = ["replace:hihi"]

                [[stored_metadata_rules]]
                matcher = "trackartist[main]:haha"
                actions = ["replace:bibi", "split: "]
                ignore = ["releasetitle:blabla"]

                [path_templates]
                default.release = "{{{{ title }}}}"
                default.track = "{{{{ title }}}}"
                default.all_tracks = "{{{{ title }}}}"
                source.release = "{{{{ title }}}}"
                source.track = "{{{{ title }}}}"
                source.all_tracks = "{{{{ title }}}}"
                releases.release = "{{{{ title }}}}"
                releases.track = "{{{{ title }}}}"
                releases.all_tracks = "{{{{ title }}}}"
                releases_new.release = "{{{{ title }}}}"
                releases_new.track = "{{{{ title }}}}"
                releases_new.all_tracks = "{{{{ title }}}}"
                releases_added_on.release = "{{{{ title }}}}"
                releases_added_on.track = "{{{{ title }}}}"
                releases_added_on.all_tracks = "{{{{ title }}}}"
                releases_released_on.release = "{{{{ title }}}}"
                releases_released_on.track = "{{{{ title }}}}"
                releases_released_on.all_tracks = "{{{{ title }}}}"
                artists.release = "{{{{ title }}}}"
                artists.track = "{{{{ title }}}}"
                artists.all_tracks = "{{{{ title }}}}"
                labels.release = "{{{{ title }}}}"
                labels.track = "{{{{ title }}}}"
                labels.all_tracks = "{{{{ title }}}}"
                loose_tracks.release = "{{{{ title }}}}"
                loose_tracks.track = "{{{{ title }}}}"
                loose_tracks.all_tracks = "{{{{ title }}}}"
                collages.release = "{{{{ title }}}}"
                collages.track = "{{{{ title }}}}"
                collages.all_tracks = "{{{{ title }}}}"
                # Genres and descriptors omitted to test the defaults.
                playlists = "{{{{ title }}}}"

                [vfs]
                mount_dir = "~/music"
                artists_blacklist = [ "www" ]
                genres_blacklist = [ "xxx" ]
                descriptors_blacklist = [ "yyy" ]
                labels_blacklist = [ "zzz" ]
                hide_genres_with_only_new_releases = true
                hide_descriptors_with_only_new_releases = true
                hide_labels_with_only_new_releases = true
                """
            )

        c = Config.parse(config_path_override=path)
        assert c == Config(
            music_source_dir=Path.home() / ".music-src",
            cache_dir=cache_dir,
            max_proc=8,
            artist_aliases_map={
                "Abakus": ["Cinnamon Chasers"],
                "tripleS": [
                    "EVOLution",
                    "LOVElution",
                    "+(KR)ystal Eyes",
                    "Acid Angel From Asia",
                    "Acid Eyes",
                ],
            },
            artist_aliases_parents_map={
                "Cinnamon Chasers": ["Abakus"],
                "EVOLution": ["tripleS"],
                "LOVElution": ["tripleS"],
                "+(KR)ystal Eyes": ["tripleS"],
                "Acid Angel From Asia": ["tripleS"],
                "Acid Eyes": ["tripleS"],
            },
            cover_art_stems=["aa", "bb"],
            valid_art_exts=["tiff"],
            write_parent_genres=True,
            max_filename_bytes=255,
            rename_source_files=True,
            path_templates=PathTemplateConfig(
                source=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases_new=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases_added_on=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases_released_on=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                artists=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                genres=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                descriptors=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                labels=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                loose_tracks=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                collages=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                playlists=PathTemplate("{{ title }}"),
            ),
            ignore_release_directories=["dummy boy"],
            stored_metadata_rules=[
                Rule(
                    matcher=Matcher(["tracktitle"], Pattern("lala")),
                    actions=[
                        Action(
                            behavior=ReplaceAction(replacement="hihi"),
                            tags=["tracktitle"],
                            pattern=Pattern("lala"),
                        )
                    ],
                    ignore=[],
                ),
                Rule(
                    matcher=Matcher(["trackartist[main]"], Pattern("haha")),
                    actions=[
                        Action(
                            behavior=ReplaceAction(replacement="bibi"),
                            tags=["trackartist[main]"],
                            pattern=Pattern("haha"),
                        ),
                        Action(
                            behavior=SplitAction(delimiter=" "),
                            tags=["trackartist[main]"],
                            pattern=Pattern("haha"),
                        ),
                    ],
                    ignore=[Matcher(["releasetitle"], Pattern("blabla"))],
                ),
            ],
            vfs=VirtualFSConfig(
                mount_dir=Path.home() / "music",
                artists_whitelist=None,
                genres_whitelist=None,
                descriptors_whitelist=None,
                labels_whitelist=None,
                hide_genres_with_only_new_releases=True,
                hide_descriptors_with_only_new_releases=True,
                hide_labels_with_only_new_releases=True,
                artists_blacklist=["www"],
                genres_blacklist=["xxx"],
                descriptors_blacklist=["yyy"],
                labels_blacklist=["zzz"],
            ),
        )


def test_config_whitelist() -> None:
    """Since whitelist and blacklist are mutually exclusive, we can't test them in the same test."""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """
                music_source_dir = "~/.music-src"
                vfs.mount_dir = "~/music"
                vfs.artists_whitelist = [ "www" ]
                vfs.genres_whitelist = [ "xxx" ]
                vfs.descriptors_whitelist = [ "yyy" ]
                vfs.labels_whitelist = [ "zzz" ]
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.vfs.artists_whitelist == ["www"]
        assert c.vfs.genres_whitelist == ["xxx"]
        assert c.vfs.descriptors_whitelist == ["yyy"]
        assert c.vfs.labels_whitelist == ["zzz"]
        assert c.vfs.artists_blacklist is None
        assert c.vfs.genres_blacklist is None
        assert c.vfs.descriptors_blacklist is None
        assert c.vfs.labels_blacklist is None


def test_config_not_found() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with pytest.raises(ConfigNotFoundError):
            Config.parse(config_path_override=path)


def test_config_missing_key_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def append(x: str) -> None:
            with path.open("a") as fp:
                fp.write("\n" + x)

        append('music_source_dir = "/"')
        with pytest.raises(MissingConfigKeyError) as excinfo:
            Config.parse(config_path_override=path)
        assert str(excinfo.value) == f"Missing key vfs.mount_dir in configuration file ({path})"


def test_config_value_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def write(x: str) -> None:
            with path.open("w") as fp:
                fp.write(x)

        config = ""

        # music_source_dir
        write("music_source_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value) == f"Invalid value for music_source_dir in configuration file ({path}): must be a path"
        )
        config += '\nmusic_source_dir = "~/.music-src"'

        # cache_dir
        write(config + "\ncache_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert str(excinfo.value) == f"Invalid value for cache_dir in configuration file ({path}): must be a path"
        config += '\ncache_dir = "~/.cache/rose"'

        # max_proc
        write(config + '\nmax_proc = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for max_proc in configuration file ({path}): must be a positive integer"
        )
        config += "\nmax_proc = 8"

        # artist_aliases
        write(config + '\nartist_aliases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = [["lalala"]]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases="lalala"}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases=[123]}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        config += '\nartist_aliases = [{artist="tripleS", aliases=["EVOLution"]}]'

        # cover_art_stems
        write(config + '\ncover_art_stems = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cover_art_stems in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ncover_art_stems = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cover_art_stems in configuration file ({path}): Each cover art stem must be of type str: got <class 'int'>"
        )
        config += '\ncover_art_stems = [ "cover" ]'

        # valid_art_exts
        write(config + '\nvalid_art_exts = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for valid_art_exts in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nvalid_art_exts = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for valid_art_exts in configuration file ({path}): Each art extension must be of type str: got <class 'int'>"
        )
        config += '\nvalid_art_exts = [ "jpg" ]'

        # write_parent_genres
        write(config + '\nwrite_parent_genres = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for write_parent_genres in configuration file ({path}): Must be a bool: got <class 'str'>"
        )

        # max_filename_bytes
        write(config + '\nmax_filename_bytes = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for max_filename_bytes in configuration file ({path}): Must be an int: got <class 'str'>"
        )
        config += "\nmax_filename_bytes = 240"

        # ignore_release_directories
        write(config + '\nignore_release_directories = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for ignore_release_directories in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nignore_release_directories = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for ignore_release_directories in configuration file ({path}): Each release directory must be of type str: got <class 'int'>"
        )
        config += '\nignore_release_directories = [ ".stversions" ]'

        # stored_metadata_rules
        write(config + '\nstored_metadata_rules = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value in stored_metadata_rules in configuration file ({path}): list values must be a dict: got <class 'str'>"
        )
        write(config + '\nstored_metadata_rules = [{ matcher = "tracktitle:hi", actions = ["delete:hi"] }]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            click.unstyle(str(excinfo.value))
            == f"""\
Failed to parse stored_metadata_rules in configuration file ({path}): rule {{'matcher': 'tracktitle:hi', 'actions': ['delete:hi']}}: Failed to parse action 1, invalid syntax:

    delete:hi
           ^
           Found another section after the action kind, but the delete action has no parameters. Please remove this section.
"""
        )
        write(
            config
            + '\nstored_metadata_rules = [{ matcher = "tracktitle:hi", actions = ["delete"], ignore = ["tracktitle:bye:"] }]'
        )
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            click.unstyle(str(excinfo.value))
            == f"""\
Failed to parse stored_metadata_rules in configuration file ({path}): rule {{'matcher': 'tracktitle:hi', 'actions': ['delete'], 'ignore': ['tracktitle:bye:']}}: Failed to parse ignore, invalid syntax:

    tracktitle:bye:
                   ^
                   No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).
"""
        )

        # rename_source_files
        write(config + '\nrename_source_files = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for rename_source_files in configuration file ({path}): Must be a bool: got <class 'str'>"
        )


def test_vfs_config_value_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def write(x: str) -> None:
            with path.open("w") as fp:
                fp.write(x)

        config = 'music_source_dir = "~/.music-src"\n[vfs]\n'
        write(config)

        # mount_dir
        write(config + "\nmount_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert str(excinfo.value) == f"Invalid value for vfs.mount_dir in configuration file ({path}): must be a path"
        config += '\nmount_dir = "~/music"'

        # artists_whitelist
        write(config + '\nartists_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nartists_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_whitelist in configuration file ({path}): Each artist must be of type str: got <class 'int'>"
        )

        # genres_whitelist
        write(config + '\ngenres_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ngenres_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_whitelist in configuration file ({path}): Each genre must be of type str: got <class 'int'>"
        )

        # labels_whitelist
        write(config + '\nlabels_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nlabels_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_whitelist in configuration file ({path}): Each label must be of type str: got <class 'int'>"
        )

        # artists_blacklist
        write(config + '\nartists_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nartists_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_blacklist in configuration file ({path}): Each artist must be of type str: got <class 'int'>"
        )

        # genres_blacklist
        write(config + '\ngenres_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ngenres_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_blacklist in configuration file ({path}): Each genre must be of type str: got <class 'int'>"
        )

        # descriptors_blacklist
        write(config + '\ndescriptors_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.descriptors_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ndescriptors_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.descriptors_blacklist in configuration file ({path}): Each descriptor must be of type str: got <class 'int'>"
        )

        # labels_blacklist
        write(config + '\nlabels_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nlabels_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_blacklist in configuration file ({path}): Each label must be of type str: got <class 'int'>"
        )

        # artists_whitelist + artists_blacklist
        write(config + '\nartists_whitelist = ["a"]\nartists_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both vfs.artists_whitelist and vfs.artists_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # genres_whitelist + genres_blacklist
        write(config + '\ngenres_whitelist = ["a"]\ngenres_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both vfs.genres_whitelist and vfs.genres_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # labels_whitelist + labels_blacklist
        write(config + '\nlabels_whitelist = ["a"]\nlabels_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both vfs.labels_whitelist and vfs.labels_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # hide_genres_with_only_new_releases
        write(config + '\nhide_genres_with_only_new_releases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.hide_genres_with_only_new_releases in configuration file ({path}): Must be a bool: got <class 'str'>"
        )

        # hide_descriptors_with_only_new_releases
        write(config + '\nhide_descriptors_with_only_new_releases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.hide_descriptors_with_only_new_releases in configuration file ({path}): Must be a bool: got <class 'str'>"
        )

        # hide_labels_with_only_new_releases
        write(config + '\nhide_labels_with_only_new_releases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.hide_labels_with_only_new_releases in configuration file ({path}): Must be a bool: got <class 'str'>"
        )
