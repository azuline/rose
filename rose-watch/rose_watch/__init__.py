from rose import initialize_logging

from rose_watch.watcher import start_watchdog

__all__ = [
    "start_watchdog",
]

initialize_logging(__name__)
