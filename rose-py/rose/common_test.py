from rose.common import sanitize_dirname, sanitize_filename
from rose.config import Config


def test_sanitize_diacritics(config: Config):
    assert sanitize_dirname(config, "Préludes", False, sanitize_diacritics=True) == "Preludes"
    assert sanitize_filename(config, "Préludes", False, sanitize_diacritics=True) == "Preludes"
