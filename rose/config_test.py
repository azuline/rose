import tempfile
from pathlib import Path

import click
import pytest

from rose.config import (
    Config,
    ConfigNotFoundError,
    InvalidConfigValueError,
    MissingConfigKeyError,
)
from rose.rule_parser import (
    MatcherPattern,
    MetadataAction,
    MetadataMatcher,
    MetadataRule,
    ReplaceAction,
    SplitAction,
)
from rose.templates import PathTemplate, PathTemplateConfig, PathTemplatePair


def test_config_minimal() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """
                music_source_dir = "~/.music-src"
                fuse_mount_dir = "~/music"
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.music_source_dir == Path.home() / ".music-src"
        assert c.fuse_mount_dir == Path.home() / "music"


def test_config_full() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        cache_dir = Path(tmpdir) / "cache"
        with path.open("w") as fp:
            fp.write(
                f"""
                music_source_dir = "~/.music-src"
                fuse_mount_dir = "~/music"
                cache_dir = "{cache_dir}"
                max_proc = 8
                artist_aliases = [
                  {{ artist = "Abakus", aliases = ["Cinnamon Chasers"] }},
                  {{ artist = "tripleS", aliases = ["EVOLution", "LOVElution", "+(KR)ystal Eyes", "Acid Angel From Asia", "Acid Eyes"] }},
                ]
                fuse_artists_blacklist = [ "www" ]
                fuse_genres_blacklist = [ "xxx" ]
                fuse_descriptors_blacklist = [ "yyy" ]
                fuse_labels_blacklist = [ "zzz" ]
                cover_art_stems = [ "aa", "bb" ]
                valid_art_exts = [ "tiff" ]
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
                source.release = "{{{{ title }}}}"
                source.track = "{{{{ title }}}}"
                releases.release = "{{{{ title }}}}"
                releases.track = "{{{{ title }}}}"
                releases_new.release = "{{{{ title }}}}"
                releases_new.track = "{{{{ title }}}}"
                releases_added_on.release = "{{{{ title }}}}"
                releases_added_on.track = "{{{{ title }}}}"
                releases_released_on.release = "{{{{ title }}}}"
                releases_released_on.track = "{{{{ title }}}}"
                artists.release = "{{{{ title }}}}"
                artists.track = "{{{{ title }}}}"
                labels.release = "{{{{ title }}}}"
                labels.track = "{{{{ title }}}}"
                collages.release = "{{{{ title }}}}"
                collages.track = "{{{{ title }}}}"
                playlists = "{{{{ title }}}}"
                """
            )

        c = Config.parse(config_path_override=path)
        assert c == Config(
            music_source_dir=Path.home() / ".music-src",
            fuse_mount_dir=Path.home() / "music",
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
            fuse_artists_whitelist=None,
            fuse_genres_whitelist=None,
            fuse_descriptors_whitelist=None,
            fuse_labels_whitelist=None,
            fuse_artists_blacklist=["www"],
            fuse_genres_blacklist=["xxx"],
            fuse_descriptors_blacklist=["yyy"],
            fuse_labels_blacklist=["zzz"],
            cover_art_stems=["aa", "bb"],
            valid_art_exts=["tiff"],
            max_filename_bytes=255,
            rename_source_files=True,
            path_templates=PathTemplateConfig(
                source=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                releases=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                releases_new=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                releases_added_on=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                releases_released_on=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                artists=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                genres=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                descriptors=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                labels=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                collages=PathTemplatePair(
                    release=PathTemplate("{{ title }}"), track=PathTemplate("{{ title }}")
                ),
                playlists=PathTemplate("{{ title }}"),
            ),
            ignore_release_directories=["dummy boy"],
            stored_metadata_rules=[
                MetadataRule(
                    matcher=MetadataMatcher(tags=["tracktitle"], pattern=MatcherPattern("lala")),
                    actions=[
                        MetadataAction(
                            behavior=ReplaceAction(replacement="hihi"),
                            tags=["tracktitle"],
                            pattern=MatcherPattern("lala"),
                        )
                    ],
                    ignore=[],
                ),
                MetadataRule(
                    matcher=MetadataMatcher(
                        tags=["trackartist[main]"], pattern=MatcherPattern("haha")
                    ),
                    actions=[
                        MetadataAction(
                            behavior=ReplaceAction(replacement="bibi"),
                            tags=["trackartist[main]"],
                            pattern=MatcherPattern("haha"),
                        ),
                        MetadataAction(
                            behavior=SplitAction(delimiter=" "),
                            tags=["trackartist[main]"],
                            pattern=MatcherPattern("haha"),
                        ),
                    ],
                    ignore=[
                        MetadataMatcher(tags=["releasetitle"], pattern=MatcherPattern("blabla"))
                    ],
                ),
            ],
        )


def test_config_whitelist() -> None:
    """Since whitelist and blacklist are mutually exclusive, we can't test them in the same test."""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """
                music_source_dir = "~/.music-src"
                fuse_mount_dir = "~/music"
                fuse_artists_whitelist = [ "www" ]
                fuse_genres_whitelist = [ "xxx" ]
                fuse_descriptors_whitelist = [ "yyy" ]
                fuse_labels_whitelist = [ "zzz" ]
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.fuse_artists_whitelist == ["www"]
        assert c.fuse_genres_whitelist == ["xxx"]
        assert c.fuse_descriptors_whitelist == ["yyy"]
        assert c.fuse_labels_whitelist == ["zzz"]
        assert c.fuse_artists_blacklist is None
        assert c.fuse_genres_blacklist is None
        assert c.fuse_descriptors_blacklist is None
        assert c.fuse_labels_blacklist is None


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
        assert str(excinfo.value) == f"Missing key fuse_mount_dir in configuration file ({path})"


def test_config_value_validation() -> None:
    config = ""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def write(x: str) -> None:
            with path.open("w") as fp:
                fp.write(x)

        # music_source_dir
        write("music_source_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for music_source_dir in configuration file ({path}): must be a path"
        )
        config += '\nmusic_source_dir = "~/.music-src"'

        # fuse_mount_dir
        write(config + "\nfuse_mount_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_mount_dir in configuration file ({path}): must be a path"
        )
        config += '\nfuse_mount_dir = "~/music"'

        # cache_dir
        write(config + "\ncache_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cache_dir in configuration file ({path}): must be a path"
        )
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

        # fuse_artists_whitelist
        write(config + '\nfuse_artists_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_artists_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_whitelist in configuration file ({path}): Each artist must be of type str: got <class 'int'>"
        )

        # fuse_genres_whitelist
        write(config + '\nfuse_genres_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_genres_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_whitelist in configuration file ({path}): Each genre must be of type str: got <class 'int'>"
        )

        # fuse_labels_whitelist
        write(config + '\nfuse_labels_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_labels_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_whitelist in configuration file ({path}): Each label must be of type str: got <class 'int'>"
        )

        # fuse_artists_blacklist
        write(config + '\nfuse_artists_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_artists_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_blacklist in configuration file ({path}): Each artist must be of type str: got <class 'int'>"
        )

        # fuse_genres_blacklist
        write(config + '\nfuse_genres_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_genres_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_blacklist in configuration file ({path}): Each genre must be of type str: got <class 'int'>"
        )

        # fuse_descriptors_blacklist
        write(config + '\nfuse_descriptors_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_descriptors_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_descriptors_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_descriptors_blacklist in configuration file ({path}): Each descriptor must be of type str: got <class 'int'>"
        )

        # fuse_labels_blacklist
        write(config + '\nfuse_labels_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nfuse_labels_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_blacklist in configuration file ({path}): Each label must be of type str: got <class 'int'>"
        )

        # fuse_artists_whitelist + fuse_artists_blacklist
        write(config + '\nfuse_artists_whitelist = ["a"]\nfuse_artists_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both fuse_artists_whitelist and fuse_artists_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # fuse_genres_whitelist + fuse_genres_blacklist
        write(config + '\nfuse_genres_whitelist = ["a"]\nfuse_genres_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both fuse_genres_whitelist and fuse_genres_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # fuse_labels_whitelist + fuse_labels_blacklist
        write(config + '\nfuse_labels_whitelist = ["a"]\nfuse_labels_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both fuse_labels_whitelist and fuse_labels_blacklist in configuration file ({path}): must specify only one or the other"
        )

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
        write(
            config
            + '\nstored_metadata_rules = [{ matcher = "tracktitle:hi", actions = ["delete:hi"] }]'
        )
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

        # path_templates
        write(config + '\npath_templates.source.release = "{% if hi %}{{"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid path template in configuration file ({path}) for template source.release: Failed to compile template: unexpected 'end of template'"
        )

        # rename_source_files
        write(config + '\nrename_source_files = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for rename_source_files in configuration file ({path}): Must be a bool: got <class 'str'>"
        )
