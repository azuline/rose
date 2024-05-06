import click
from click.testing import CliRunner

from rose.config import Config
from rose_cli.templates import (
    preview_path_templates,
)


def test_preview_templates(config: Config) -> None:
    runner = CliRunner()
    with runner.isolated_filesystem(), runner.isolation() as out_streams:
        preview_path_templates(config)
        out_streams[0].seek(0)
        output = click.unstyle(out_streams[0].read().decode())

    assert (
        output
        == """\
Source Directory - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
Source Directory - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

1. Releases - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
1. Releases - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

1. Releases (New) - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
1. Releases (New) - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

1. Releases (Added On) - Release:
  Sample 1: [2023-04-20] Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: [2023-06-09] BTS - 2016. Young Forever (花樣年華)
  Sample 3: [2023-09-06] Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
1. Releases (Added On) - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

1. Releases (Released On) - Release:
  Sample 1: [2023-04-20] Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: [2023-06-09] BTS - 2016. Young Forever (花樣年華)
  Sample 3: [2023-09-06] Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
1. Releases (Released On) - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

2. Artists - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
2. Artists - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

3. Genres - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
3. Genres - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

4. Descriptors - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
4. Descriptors - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

5. Labels - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
5. Labels - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

6. Collages - Release:
  Sample 1: 1. Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: 2. BTS - 2016. Young Forever (花樣年華)
  Sample 3: 3. Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
6. Collages - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus

7. Playlists - Track:
  Sample 1: 1. Kim Lip - Eclipse.opus
  Sample 2: 2. BTS - House of Cards.opus
  Sample 3: 3. Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - Gigues: Modéré.opus
"""
    )
