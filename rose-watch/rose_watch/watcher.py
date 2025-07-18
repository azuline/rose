"""
The watcher module is architected to decouple the event listener from the event processor.

This is because we must introduce a wait into the event processor, but we do not want to block the
event listener. In order to performantly introduce such a wait, we've made the event processor
async. However, the event listener uses a non-async library, so the event listener is sync.

Why do we need to introduce a wait time into the event processor? Changes to releases occur across
an entire directory, but change events come in one file at a time. If we respond to a file event too
early, we may do silly things like write a new `.rose.{uuid}.toml` file, only to have a pre-existing
one suddenly appear after. We only want to operate on a release once all files have finished
changing.

Let's dive down into the details. The watcher is a process that spawns a background thread and then
runs an event loop. Hierarchically:

Process
    Thread -> watchdog/inotify listener that enqueues events
    Event Loop -> processes+debounces events asynchronously from the queue
"""

import asyncio
import contextlib
import logging
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from queue import Empty, Queue
from typing import Literal, cast

from rose import (
    Config,
    update_cache_evict_nonexistent_collages,
    update_cache_evict_nonexistent_playlists,
    update_cache_evict_nonexistent_releases,
    update_cache_for_collages,
    update_cache_for_playlists,
    update_cache_for_releases,
)
from watchdog.events import (
    FileSystemEvent,
    FileSystemEventHandler,
    FileSystemMovedEvent,
)
from watchdog.observers import Observer

logger = logging.getLogger(__name__)

# Shorten wait times if we are in a test. This way a test runs faster. This is wasteful in
# production though.
WAIT_DIVIDER = 1 if "pytest" not in sys.modules else 10


EventType = Literal["created", "deleted", "modified", "moved"]
EVENT_TYPES: list[EventType] = ["created", "deleted", "modified", "moved"]


@dataclass(frozen=True)
class WatchdogEvent:
    type: EventType
    collage: str | None = None
    playlist: str | None = None
    release: Path | None = None


class EventHandler(FileSystemEventHandler):  # pragma: no cover
    def __init__(self, config: Config, queue: Queue[WatchdogEvent]):
        super().__init__()
        self.config = config
        self.queue = queue

    def on_any_event(self, event: FileSystemEvent) -> None:
        super().on_any_event(event)
        path = event.dest_path if isinstance(event, FileSystemMovedEvent) else event.src_path
        if isinstance(path, bytes):
            path = path.decode()
        logger.debug(f"Notified of {event.event_type} event for {path}")

        etype = cast(EventType, event.event_type)
        if etype not in EVENT_TYPES:
            return

        # Collage event.
        relative_path = path.removeprefix(str(self.config.music_source_dir) + "/")
        if relative_path.startswith("!collages/"):
            if not relative_path.endswith(".toml"):
                return
            collage = relative_path.removeprefix("!collages/").removesuffix(".toml")
            logger.debug(f"Queueing {etype} event on collage {collage}")
            self.queue.put(WatchdogEvent(collage=collage, type=etype))
            return

        # Playlist event.
        if relative_path.startswith("!playlists/"):
            if not relative_path.endswith(".toml"):
                return
            playlist = relative_path.removeprefix("!playlists/").removesuffix(".toml")
            logger.debug(f"Queueing {etype} event on playlist {playlist}")
            self.queue.put(WatchdogEvent(playlist=playlist, type=etype))
            return

        # Release event.
        with contextlib.suppress(IndexError):
            final_path_part = Path(relative_path).parts[0]
            if final_path_part == "/":
                return
            release_dir = self.config.music_source_dir / final_path_part
            logger.debug(f"Queueing {etype} event on release {release_dir}")
            self.queue.put(WatchdogEvent(release=release_dir, type=etype))


async def handle_event(
    c: Config,
    e: WatchdogEvent,
    wait: float | None = None,
) -> None:  # pragma: no cover
    if wait:
        await asyncio.sleep(wait / WAIT_DIVIDER)

    if e.type == "created" or e.type == "modified":
        if e.collage:
            update_cache_for_collages(c, [e.collage])
        elif e.playlist:
            update_cache_for_playlists(c, [e.playlist])
        elif e.release:
            update_cache_for_releases(c, [e.release])
    elif e.type == "deleted":
        if e.collage:
            update_cache_evict_nonexistent_collages(c)
        elif e.playlist:
            update_cache_evict_nonexistent_playlists(c)
        elif e.release:
            update_cache_evict_nonexistent_releases(c)
    elif e.type == "moved":
        if e.collage:
            update_cache_for_collages(c, [e.collage])
            update_cache_evict_nonexistent_collages(c)
        elif e.playlist:
            update_cache_for_playlists(c, [e.playlist])
            update_cache_evict_nonexistent_playlists(c)
        elif e.release:
            update_cache_for_releases(c, [e.release])
            update_cache_evict_nonexistent_releases(c)


async def event_processor(c: Config, queue: Queue[WatchdogEvent]) -> None:  # pragma: no cover
    debounce_times: dict[int, float] = {}
    while True:
        if queue.empty():
            await asyncio.sleep(0.5 / WAIT_DIVIDER)

        try:
            event = queue.get(block=False)
        except Empty:
            continue

        # Debounce events.
        key = hash(event)
        last = debounce_times.get(key)
        if last and time.time() - last < 0.2:
            logger.debug(f"Skipped event {key} due to debouncer")
            continue
        debounce_times[key] = time.time()

        if event.collage:
            logger.debug(f"Updating cache in response to {event.type} event on collage {event.collage}")
            await handle_event(c, event)
            continue

        if event.playlist:
            logger.debug(f"Updating cache in response to {event.type} event on playlist {event.playlist}")
            await handle_event(c, event)
            continue

        assert event.release is not None
        # Launch the handler with the sleep asynchronously. This allows us to not block the main
        # thread, but insert a delay before processing the release.
        logger.debug(f"Updating cache in response to {event.type} event on release {event.release.name}")
        asyncio.create_task(handle_event(c, event, 2))


def start_watchdog(c: Config) -> None:  # pragma: no cover
    queue: Queue[WatchdogEvent] = Queue()
    observer = Observer()
    event_handler = EventHandler(c, queue)
    observer.schedule(event_handler, str(c.music_source_dir), recursive=True)
    logger.info("Starting watchdog filesystem event listener")
    observer.start()
    logger.info("Starting watchdog asynchronous event processor")
    asyncio.run(event_processor(c, queue))
