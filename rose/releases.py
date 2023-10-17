import json
from dataclasses import asdict
from pathlib import Path
from typing import Any

from rose.cache import list_releases
from rose.config import Config


class CustomJSONEncoder(json.JSONEncoder):
    def default(self, obj: Any) -> Any:
        if isinstance(obj, Path):
            return str(obj)
        return super().default(obj)


def print_releases(c: Config) -> None:
    releases = [asdict(r) for r in list_releases(c)]
    print(json.dumps(releases, cls=CustomJSONEncoder))
