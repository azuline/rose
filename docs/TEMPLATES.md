# Directory and Filename Templates

Rosé supports user-defined templates for configuring the directory and file names for each view of
the virtual filesystem and for the source directory.

> [!NOTE]
> The source directory templates are only applied to the source directory if the
> `rename_source_files` configuration option is true. See [Configuration](./CONFIGURATION.md) for
> more details.

To customize the templates, define them in your configuration file. The configuration keys for
templates are:

```toml
[path_templates]
default.release = "..."
default.track = "..."
default.all_tracks = "..."
source.release = "..."
source.track = "..."
source.all_tracks = "..."
releases.release = "..."
releases.track = "..."
releases.all_tracks = "..."
releases_new.release = "..."
releases_new.track = "..."
releases_new.all_tracks = "..."
releases_added_on.release = "..."
releases_added_on.track = "..."
releases_added_on.all_tracks = "..."
releases_released_on.release = "..."
releases_released_on.track = "..."
releases_released_on.all_tracks = "..."
artists.release = "..."
artists.track = "..."
artists.all_tracks = "..."
genres.release = "..."
genres.track = "..."
genres.all_tracks = "..."
descriptors.release = "..."
descriptors.track = "..."
descriptors.all_tracks = "..."
labels.release = "..."
labels.track = "..."
labels.all_tracks = "..."
collages.release = "..."
collages.track = "..."
collages.all_tracks = "..."
playlists = "..."
```

If set, the `default.xxx` templates are used as the default values for all other unset templates
(except playlist). Otherwise the templates default to:

```jinja2
{# "Default Default" Release Template #}

{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}

{# "Default Default" Track Template #}

{% if disctotal > 1 %}{{ discnumber.rjust(2, '0') }}-{% endif %}{{ tracknumber.rjust(2, '0') }}.
{{ tracktitle }}
{% if trackartists.guest %}(feat. {{ trackartists.guest | artistsarrayfmt }}){% endif %}

{# "Default Default" All Tracks Template #}

{{ trackartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }} -
{{ tracktitle }}
```

# Template Language

Rosé uses the Jinja templating language. See [Jinja's Template Designer
Documentation](https://jinja.palletsprojects.com/en/3.1.x/templates/).

After evaluating the template, Rosé replaces all adjacent whitespace characters (e.g. space, tab,
newline, return, etc.) with a single space. This allows you to freely use multiple lines and
comments when defining your templates, like so:

```toml
[path_templates]
default.release = """
  {{ releaseartists | artistsfmt }} -
  {% if releasedate %}{{ releasedate }}.{% endif %}         {# Hi! This is a comment! #}
  {{ releasetitle }}
  {% if new %}[NEW]{% endif %}
"""
```

Rosé provides the following template variables for releases:

```python
added_at: str                        # ISO8601 timestamp of when the release was added to the library.
releasetitle: str
releasetype: str                     # The type of the release (e.g. single, ep, etc). One of the enums as defined in TAGGING_CONVENTIONS.md.
releasedate: int | None              # The year of this edition of the release.
originaldate: int | None             # The year of the first edition of the release.
compositiondate: int | None          # The year that the release was composed. Mainly of interest in classical music.
new: bool                            # The "new"-ness of the release. See RELEASES.md for documentation on this feature.
disctotal: int                       # The number of discs in the release.
genres: list[str]
parent_genres: list[str]             # The parent genres of `genres`, excluding `genres`.
secondary_genres: list[str]          # The secondary/minor genres.
parent_secondary_genres: list[str]   # The parent genres of `secondary_genres`, excluding `secondary_genres`.
labels: list[str]
catalognumber: str | None
edition: str | None                  # The title of this edition of the release.
releaseartists: ArtistMapping        # All release artists: an object with 6 properties, each corresponding to one role.
releaseartists.main: list[Artist]    # The Artist object has a `name` property with the artist name.
releaseartists.guest: list[Artist]
releaseartists.remixer: list[Artist]
releaseartists.producer: list[Artist]
releaseartists.composer: list[Artist]
releaseartists.conductor: list[Artist]
releaseartists.djmixer: list[Artist]
position: str                        # If in a collage context, the zero-padded position of the release in the collage.
context.genre: str                   # The current genre being viewed in the Virtual Filesystem.
context.label: str                   # The current label being viewed in the Virtual Filesystem.
context.artist: str                  # The current artist being viewed in the Virtual Filesystem.
context.collage: str                 # The current collage being viewed in the Virtual Filesystem.
context.playlist: str                # The current playlist being viewed in the Virtual Filesystem.
```

And provides the template variables for tracks:

```python
added_at: str                        # ISO8601 timestamp of when the track's parent release was added to the library.
tracktitle: str
tracknumber: str
tracktotal: int                      # The number of tracks on this disc.
discnumber: str
disctotal: int                       # The number of discs in the release.
duration_seconds: int
trackartists: ArtistMapping          # All track artists: an object with 6 properties, each corresponding to one role.
trackartists.main: list[Artist]      # The Artist object has a `name` property with the artist name.
trackartists.guest: list[Artist]
trackartists.remixer: list[Artist]
trackartists.producer: list[Artist]
trackartists.composer: list[Artist]
trackartists.conductor: list[Artist]
trackartists.djmixer: list[Artist]
releasetitle: str
releasetype: str                     # The type of the track's release (e.g. single, ep, etc).
releasedate: int | None
originaldate: int | None             # The year of the first edition of the release.
compositiondate: int | None          # The year that the release was composed. Mainly of interest in classical music.
new: bool                            # The "new"-ness of the track's release.
genres: list[str]
parent_genres: list[str]             # The parent genres of `genres`, excluding `genres`.
secondary_genres: list[str]          # The secondary/minor genres.
parent_secondary_genres: list[str]   # The parent genres of `secondary_genres`, excluding `secondary_genres`.
labels: list[str]
catalognumber: str | None
edition: str | None                  # The title of this edition of the release.
releaseartists: ArtistMapping        # All release artists: an object with 6 properties, each corresponding to one role.
releaseartists.main: list[Artist]    # The Artist object has a `name` property with the artist name.
releaseartists.guest: list[Artist]
releaseartists.remixer: list[Artist]
releaseartists.producer: list[Artist]
releaseartists.composer: list[Artist]
releaseartists.conductor: list[Artist]
releaseartists.djmixer: list[Artist]
position: str                        # If in a playlist context, the zero-padded position of the track in the playlist.
context.genre: str                   # The current genre being viewed in the Virtual Filesystem.
context.label: str                   # The current label being viewed in the Virtual Filesystem.
context.artist: str                  # The current artist being viewed in the Virtual Filesystem.
context.collage: str                 # The current collage being viewed in the Virtual Filesystem.
context.playlist: str                # The current playlist being viewed in the Virtual Filesystem.
```

Rosé also provides the following custom filters:

```python
arrayfmt: (list[str]) -> str            # Formats an array of strings as x, y & z.
artistsarrayfmt: (list[Artist]) -> str  # Formats an array of Artist objects as x, y & z.
artistsfmt: ArtistMapping -> str        # Formats an ArtistMapping; puts guests in (feat. x) and producers in (prod. x).
releasetypefmt: str -> str              # Correctly capitalizes the all-lowercase release type enum value.
sortorder: str -> str                   # Formats an artist name as Lastname, Firstname.
lastname: str -> str                    # Formats an artist name as Lastname.
```

# Previewing Templates

You can preview your templates with the following command. It will evaluate all your templates with
sample data.

```bash
$ rose config preview-templates
Preview for template Source Directory - Release:
  Sample 1: Kim Lip - 2017. Kim Lip
  Sample 2: BTS - 2016. Young Forever (花樣年華)
Preview for template Source Directory - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus

Preview for template 1. All Releases - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [K-Pop, Dance-Pop & Contemporary R&B]
  Sample 2: BTS - 2016. Young Forever (花樣年華) [K-Pop]
Preview for template 1. All Releases - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus

Preview for template 2. New Releases - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [K-Pop, Dance-Pop & Contemporary R&B]
  Sample 2: BTS - 2016. Young Forever (花樣年華) [K-Pop]
...
```

# Examples

See my templates [here](https://github.com/azuline/nixos/blob/master/home/rose/config.toml).
