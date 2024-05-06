from rose import initialize_logging
from rose_watchdog.watcher import start_watchdog

__all__ = [
    "start_watchdog",
]

initialize_logging(__name__)
