import tempfile
from pathlib import Path

import pytest

from rose.config import Config, ConfigNotFoundError, InvalidConfigValueError, MissingConfigKeyError
from rose.rule_parser import MetadataMatcher, MetadataRule, ReplaceAction


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
                fuse_artists_blacklist = [ "xxx" ]
                fuse_genres_blacklist = [ "yyy" ]
                fuse_labels_blacklist = [ "zzz" ]
                cover_art_stems = [ "aa", "bb" ]
                valid_art_exts = [ "tiff" ]
                ignore_release_directories = [ "dummy boy" ]

                [[stored_metadata_rules]]
                matcher = {{ tags = "tracktitle", pattern = "lala" }}
                action = {{ kind = "replace", replacement = "hihi" }}
                """  # noqa: E501
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
            fuse_labels_whitelist=None,
            fuse_artists_blacklist=["xxx"],
            fuse_genres_blacklist=["yyy"],
            fuse_labels_blacklist=["zzz"],
            cover_art_stems=["aa", "bb"],
            valid_art_exts=["tiff"],
            ignore_release_directories=["dummy boy"],
            stored_metadata_rules=[
                MetadataRule(
                    matcher=MetadataMatcher(tags=["tracktitle"], pattern="lala"),
                    action=ReplaceAction(replacement="hihi"),
                )
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
                fuse_artists_whitelist = [ "xxx" ]
                fuse_genres_whitelist = [ "yyy" ]
                fuse_labels_whitelist = [ "zzz" ]
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.fuse_artists_whitelist == ["xxx"]
        assert c.fuse_genres_whitelist == ["yyy"]
        assert c.fuse_labels_whitelist == ["zzz"]
        assert c.fuse_artists_blacklist is None
        assert c.fuse_genres_blacklist is None
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
            == f"Invalid value for max_proc in configuration file ({path}): must be a positive integer"  # noqa
        )
        config += "\nmax_proc = 8"

        # artist_aliases
        write(config + '\nartist_aliases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = [["lalala"]]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases="lalala"}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases=[123]}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        config += '\nartist_aliases = [{artist="tripleS", aliases=["EVOLution"]}]'

        # fuse_artists_whitelist
        write(config + '\nfuse_artists_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_whitelist in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_artists_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_whitelist in configuration file ({path}): must be a list of strings"  # noqa
        )

        # fuse_genres_whitelist
        write(config + '\nfuse_genres_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_whitelist in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_genres_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_whitelist in configuration file ({path}): must be a list of strings"  # noqa
        )

        # fuse_labels_whitelist
        write(config + '\nfuse_labels_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_whitelist in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_labels_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_whitelist in configuration file ({path}): must be a list of strings"  # noqa
        )

        # fuse_artists_blacklist
        write(config + '\nfuse_artists_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_blacklist in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_artists_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_artists_blacklist in configuration file ({path}): must be a list of strings"  # noqa
        )

        # fuse_genres_blacklist
        write(config + '\nfuse_genres_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_blacklist in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_genres_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_genres_blacklist in configuration file ({path}): must be a list of strings"  # noqa
        )

        # fuse_labels_blacklist
        write(config + '\nfuse_labels_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_blacklist in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_labels_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_labels_blacklist in configuration file ({path}): must be a list of strings"  # noqa
        )

        # fuse_artists_whitelist + fuse_artists_blacklist
        write(config + '\nfuse_artists_whitelist = ["a"]\nfuse_artists_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both fuse_artists_whitelist and fuse_artists_blacklist in configuration file ({path}): must specify only one or the other"  # noqa: E501
        )

        # fuse_genres_whitelist + fuse_genres_blacklist
        write(config + '\nfuse_genres_whitelist = ["a"]\nfuse_genres_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both fuse_genres_whitelist and fuse_genres_blacklist in configuration file ({path}): must specify only one or the other"  # noqa: E501
        )

        # fuse_labels_whitelist + fuse_labels_blacklist
        write(config + '\nfuse_labels_whitelist = ["a"]\nfuse_labels_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both fuse_labels_whitelist and fuse_labels_blacklist in configuration file ({path}): must specify only one or the other"  # noqa: E501
        )

        # cover_art_stems
        write(config + '\ncover_art_stems = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cover_art_stems in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\ncover_art_stems = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cover_art_stems in configuration file ({path}): must be a list of strings"  # noqa
        )
        config += '\ncover_art_stems = [ "cover" ]'

        # valid_art_exts
        write(config + '\nvalid_art_exts = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for valid_art_exts in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nvalid_art_exts = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for valid_art_exts in configuration file ({path}): must be a list of strings"  # noqa
        )
        config += '\nvalid_art_exts = [ "jpg" ]'

        # ignore_release_directories
        write(config + '\nignore_release_directories = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for ignore_release_directories in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nignore_release_directories = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for ignore_release_directories in configuration file ({path}): must be a list of strings"  # noqa
        )
        config += '\nignore_release_directories = [ ".stversions" ]'

        # stored_metadata_rules
        write(config + '\nstored_metadata_rules = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for stored_metadata_rules in configuration file ({path}): rule lalala could not be parsed: Type of metadata rule data must be dict: got <class 'str'>"  # noqa: E501
        )
