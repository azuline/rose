import logging
from dataclasses import dataclass
from pathlib import Path

from watchdog.events import (
    DirCreatedEvent,
    DirDeletedEvent,
    FileCreatedEvent,
    FileDeletedEvent,
    FileModifiedEvent,
    FileSystemEventHandler,
    FileSystemMovedEvent,
)
from watchdog.observers import Observer
from watchdog.observers.api import BaseObserver

from rose.cache import (
    update_cache_evict_nonexistent_collages,
    update_cache_evict_nonexistent_releases,
    update_cache_for_collages,
    update_cache_for_releases,
)
from rose.config import Config

logger = logging.getLogger(__name__)


@dataclass
class AffectedEntity:
    release: Path | None = None
    collage: str | None = None


def parse_affected_entity(config: Config, path: str) -> AffectedEntity | None:
    relative_path = path.removeprefix(str(config.music_source_dir) + "/")
    if relative_path.startswith("!collages/"):
        if not relative_path.endswith(".toml"):
            return None
        collage = relative_path.removeprefix("!collages/").removesuffix(".toml")
        logger.debug(f"Parsed change event on collage {collage}")
        return AffectedEntity(collage=collage)
    try:
        release_dir = config.music_source_dir / Path(relative_path).parts[0]
        logger.debug(f"Parsed event on release {release_dir}")
        return AffectedEntity(release=release_dir)
    except IndexError:
        return None


class EventHandler(FileSystemEventHandler):
    def __init__(self, config: Config):
        super().__init__()
        self.config = config

    def on_created(self, event: FileCreatedEvent | DirCreatedEvent) -> None:
        super().on_created(event)  # type: ignore
        logger.debug(f"Notified of change event for {event.src_path}")
        affected = parse_affected_entity(self.config, event.src_path)
        if not affected:
            return
        if affected.collage:
            update_cache_for_collages(self.config, [affected.collage])
        elif affected.release:
            update_cache_for_releases(self.config, [affected.release])

    def on_deleted(self, event: FileDeletedEvent | DirDeletedEvent) -> None:
        super().on_deleted(event)  # type: ignore
        logger.debug(f"Notified of change event for {event.src_path}")
        affected = parse_affected_entity(self.config, event.src_path)
        if not affected:
            return
        if affected.collage:
            update_cache_evict_nonexistent_collages(self.config)
        elif affected.release:
            update_cache_evict_nonexistent_releases(self.config)

    def on_modified(self, event: FileModifiedEvent) -> None:
        super().on_modified(event)  # type: ignore
        logger.debug(f"Notified of change event for {event.src_path}")
        affected = parse_affected_entity(self.config, event.src_path)
        if not affected:
            return
        if affected.collage:
            update_cache_for_collages(self.config, [affected.collage])
        elif affected.release:
            update_cache_for_releases(self.config, [affected.release])

    def on_moved(self, event: FileSystemMovedEvent) -> None:
        super().on_moved(event)  # type: ignore
        logger.debug(f"Notified of change event for {event.src_path}")
        affected = parse_affected_entity(self.config, event.dest_path)
        if not affected:
            return
        if affected.collage:
            update_cache_for_collages(self.config, [affected.collage])
            update_cache_evict_nonexistent_collages(self.config)
        elif affected.release:
            update_cache_for_releases(self.config, [affected.release])
            update_cache_evict_nonexistent_releases(self.config)


def create_watchdog_observer(c: Config) -> BaseObserver:
    observer = Observer()
    event_handler = EventHandler(c)
    observer.schedule(event_handler, c.music_source_dir, recursive=True)  # type: ignore
    return observer


def start_watchdog(c: Config, foreground: bool = False) -> None:  # pragma: no cover
    logger.info("Starting cache watchdog")
    thread = create_watchdog_observer(c)
    thread.start()  # type: ignore
    if foreground:
        thread.join()
