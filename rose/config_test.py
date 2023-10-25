import tempfile
from pathlib import Path

import pytest

from rose.config import Config, ConfigNotFoundError, InvalidConfigValueError, MissingConfigKeyError


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

        c = Config.read(config_path_override=path)
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
                fuse_hide_artists = [ "xxx" ]
                fuse_hide_genres = [ "yyy" ]
                fuse_hide_labels = [ "zzz" ]
                """  # noqa: E501
            )

        c = Config.read(config_path_override=path)
        assert c == Config(
            music_source_dir=Path.home() / ".music-src",
            fuse_mount_dir=Path.home() / "music",
            cache_dir=cache_dir,
            cache_database_path=cache_dir / "cache.sqlite3",
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
            fuse_hide_artists=["xxx"],
            fuse_hide_genres=["yyy"],
            fuse_hide_labels=["zzz"],
            hash=c.hash,
        )


def test_config_not_found() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with pytest.raises(ConfigNotFoundError):
            Config.read(config_path_override=path)


def test_config_missing_key_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def append(x: str) -> None:
            with path.open("a") as fp:
                fp.write("\n" + x)

        append('music_source_dir = "/"')
        with pytest.raises(MissingConfigKeyError) as excinfo:
            Config.read(config_path_override=path)
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
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for music_source_dir in configuration file ({path}): must be a path"
        )
        config += '\nmusic_source_dir = "~/.music-src"'

        # fuse_mount_dir
        write(config + "\nfuse_mount_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_mount_dir in configuration file ({path}): must be a path"
        )
        config += '\nfuse_mount_dir = "~/music"'

        # cache_dir
        write(config + "\ncache_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cache_dir in configuration file ({path}): must be a path"
        )
        config += '\ncache_dir = "~/.cache/rose"'

        # max_proc
        write(config + '\nmax_proc = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for max_proc in configuration file ({path}): must be a positive integer"  # noqa
        )
        config += "\nmax_proc = 8"

        # artist_aliases
        write(config + '\nartist_aliases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = [["lalala"]]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases="lalala"}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases=[123]}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"  # noqa
        )
        config += '\nartist_aliases = [{artist="tripleS", aliases=["EVOLution"]}]'

        # fuse_hide_artists
        write(config + '\nfuse_hide_artists = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_hide_artists in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_hide_artists = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_hide_artists in configuration file ({path}): must be a list of strings"  # noqa
        )
        config += '\nfuse_hide_artists = ["xxx"]'

        # fuse_hide_genres
        write(config + '\nfuse_hide_genres = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_hide_genres in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_hide_genres = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_hide_genres in configuration file ({path}): must be a list of strings"  # noqa
        )
        config += '\nfuse_hide_genres = ["xxx"]'

        # fuse_hide_labels
        write(config + '\nfuse_hide_labels = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_hide_labels in configuration file ({path}): must be a list of strings"  # noqa
        )
        write(config + "\nfuse_hide_labels = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.read(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for fuse_hide_labels in configuration file ({path}): must be a list of strings"  # noqa
        )
        config += '\nfuse_hide_labels = ["xxx"]'
