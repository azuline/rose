# Directory and Filename Templates

Rosé supports user-defined templates for configuring the directory and file
names for each view of the virtual filesystem and for the source directory.

> [!NOTE]
> The source directory templates are only applied to the source directory if
> the `rename_source_files` configuration option is true. See
> [Configuration](./CONFIGURATION.md) for more details.

To customize the templates, define them in your configuration file. The
configuration keys for templates are:

```toml
[path_templates]
default.release = "..."
default.track = "..."
source.release = "..."
source.track = "..."
all_releases.release = "..."
all_releases.track = "..."
new_releases.release = "..."
new_releases.track = "..."
recently_added_releases.release = "..."
recently_added_releases.track = "..."
artists.release = "..."
artists.track = "..."
genres.release = "..."
genres.track = "..."
labels.release = "..."
labels.track = "..."
collages.release = "..."
collages.track = "..."
playlists = "..."
```

If set, the `default.xxx` templates are used as the default values for all
other unset templates (except playlist). Otherwise the templates default to:

```jinja2
{# "Default Default" Release Template #}

{{ artists | artistsfmt }} -
{% if year %}{{ year }}.{% endif %}
{{ title }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}

{# "Default Default" Track Template #}

{% if disctotal > 1 %}{{ discnumber.rjust(2, '0') }}-{% endif %}{{ tracknumber.rjust(2, '0') }}.
{{ title }}
{% if artists.guest %}(feat. {{ artists.guest | artistsarrayfmt }}){% endif %}
```

# Template Language

Rosé uses the Jinja templating language. See [Jinja's Template Designer
Documentation](https://jinja.palletsprojects.com/en/3.1.x/templates/).

After evaluating the template, Rosé replaces all adjacent whitespace characters
(e.g. space, tab, newline, return, etc.) with a single space. This allows you
to freely use multiple lines and comments when defining your templates, like
so:

```toml
[path_templates]
default.release = """
  {{ artists | artistsfmt }} -
  {% if year %}{{ year }}.{% endif %}         {# Hi! This is a comment! #}
  {{ title }}
  {% if new %}[NEW]{% endif %}
"""
```

Rosé provides the following template variables for releases:

```python
added_at: str                   # ISO8601 timestamp of when the release was added to the library.
title: str
releasetype: str                # Type of the release (e.g. single, ep, etc). One of the enums as defined in TAGGING_CONVENTIONS.md.
year: int | None
new: bool                       # The "new"-ness of the release. See RELEASES.md for documentation on this feature.
disctotal: int                  # The number of discs in the release.
genres: list[str]
labels: list[str]
artists: ArtistMapping          # All release artists: an object with 6 properties, each corresponding to one role.
artists.main: list[Artist]      # The Artist object has a `name` property with the artist name.
artists.guest: list[Artist]
artists.remixer: list[Artist]
artists.producer: list[Artist]
artists.composer: list[Artist]
artists.djmixer: list[Artist]
position: str                   # If in a collage context, the zero-padded position of the release in the collage.
```

And provides the template variables for tracks:

```python
title: str
year: int | None
tracknumber: str
tracktotal: int                 # The number of tracks on this disc.
discnumber: str
disctotal: int                  # The number of discs in the release.
duration_seconds: int
artists: ArtistMapping          # All track artists: an object with 6 properties, each corresponding to one role.
artists.main: list[Artist]      # The Artist object has a `name` property with the artist name.
artists.guest: list[Artist]
artists.remixer: list[Artist]
artists.producer: list[Artist]
artists.composer: list[Artist]
artists.djmixer: list[Artist]
position: str                   # If in a playlist context, the zero-padded position of the track in the playlist.
```

Rosé also provides the following custom filters:

```python
arrayfmt: (list[str]) -> str               # Formats an array of strings as x, y & z.
artistsarrayfmt: (list[Artist]) -> str     # Formats an array of Artist objects as x, y & z.
artistsfmt: ArtistMapping -> str           # Formats an ArtistMapping; puts guests in (feat. x) and producers in (prod. x).
releasetypefmt: str -> str                 # Correctly capitalizes the all-lowercase release type enum value.
```

# Examples

_TODO_

# Previewing Templates

You can preview your templates with the following command. It will evaluate all
your templates with sample data.

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
