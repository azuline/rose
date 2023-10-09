import logging
import sys

logger = logging.getLogger()
logger.setLevel(logging.INFO)

# Add a logging handler for stdout unless we are testing. Pytest
# captures logging output on its own.
if "pytest" not in sys.modules:  # pragma: no cover
    stream_formatter = logging.Formatter(
        "[%(asctime)s] %(levelname)s: %(message)s",
        datefmt="%H:%M:%S",
    )
    stream_handler = logging.StreamHandler(sys.stdout)
    stream_handler.setFormatter(stream_formatter)
    logger.addHandler(stream_handler)
