import logging
import pathlib

import _pytest.pathlib

logger = logging.getLogger(__name__)


# Pytest has a bug where it doesn't handle namespace packages and treats same-name files
# in different packages as a naming collision.
#
# https://stackoverflow.com/a/72366347
# Tweaked from ^ to handle our foundation/product split.

resolve_pkg_path_orig = _pytest.pathlib.resolve_package_path
namespace_pkg_dirs = [str(d) for d in pathlib.Path(__file__).parent.iterdir() if d.is_dir()]


# patched method
def resolve_package_path(path: pathlib.Path) -> pathlib.Path | None:
    # call original lookup
    result = resolve_pkg_path_orig(path)
    if result is None:
        result = path  # let's search from the current directory upwards
    for parent in result.parents:  # pragma: no cover
        if str(parent) in namespace_pkg_dirs:
            return parent
    return None  # pragma: no cover


# apply patch
_pytest.pathlib.resolve_package_path = resolve_package_path
