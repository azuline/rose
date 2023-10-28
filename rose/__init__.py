import logging
import logging.handlers
import os
import sys
from pathlib import Path

logger = logging.getLogger()
logger.setLevel(logging.INFO)

STATE_HOME = Path(os.environ.get("XDG_STATE_HOME", "~/.local/state")).expanduser() / "rose"
STATE_HOME.mkdir(parents=True, exist_ok=True)
LOGFILE = STATE_HOME / "rose.log"

# Useful for debugging problems with the virtual FS, since pytest doesn't capture that debug logging
# output.
LOG_EVEN_THOUGH_WERE_IN_TEST = os.environ.get("LOG_TEST", False)

# Add a logging handler for stdout unless we are testing. Pytest
# captures logging output on its own, so by default, we do not attach our own.
if "pytest" not in sys.modules or LOG_EVEN_THOUGH_WERE_IN_TEST:  # pragma: no cover
    stream_template = "[%(asctime)s] %(levelname)s: %(message)s"
    if LOG_EVEN_THOUGH_WERE_IN_TEST:
        stream_template = "[ts=%(asctime)s] [pid=%(process)d] [src=%(name)s:%(lineno)s] %(levelname)s: %(message)s"  # noqa: E501
    stream_formatter = logging.Formatter(stream_template, datefmt="%H:%M:%S")
    stream_handler = logging.StreamHandler(sys.stderr)
    stream_handler.setFormatter(stream_formatter)
    logger.addHandler(stream_handler)

    file_formatter = logging.Formatter(
        "[ts=%(asctime)s] [pid=%(process)d] [src=%(name)s:%(lineno)s] %(levelname)s: %(message)s",
        datefmt="%H:%M:%S",
    )
    file_handler = logging.handlers.RotatingFileHandler(
        LOGFILE,
        maxBytes=20 * 1024 * 1024,
        backupCount=10,
    )
    file_handler.setFormatter(file_formatter)
    logger.addHandler(file_handler)
