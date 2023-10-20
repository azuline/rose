import logging
import os
import sys
from pathlib import Path

logger = logging.getLogger()
logger.setLevel(logging.INFO)

STATE_HOME = Path(os.environ.get("XDG_STATE_HOME", "~/.local/state")).expanduser() / "rose"
STATE_HOME.mkdir(parents=True, exist_ok=True)
LOGFILE = STATE_HOME / "rose.log"

# Add a logging handler for stdout unless we are testing. Pytest
# captures logging output on its own.
if True:  # "pytest" not in sys.modules:  # pragma: no cover
    stream_formatter = logging.Formatter(
        "[%(asctime)s] %(levelname)s: %(message)s",
        datefmt="%H:%M:%S",
    )
    stream_handler = logging.StreamHandler(sys.stderr)
    stream_handler.setFormatter(stream_formatter)
    logger.addHandler(stream_handler)

    file_formatter = logging.Formatter(
        "[%(asctime)s] [%(name)s:%(lineno)s] %(levelname)s: %(message)s",
        datefmt="%H:%M:%S",
    )
    file_handler = logging.FileHandler(LOGFILE)
    file_handler.setFormatter(file_formatter)
    logger.addHandler(file_handler)
