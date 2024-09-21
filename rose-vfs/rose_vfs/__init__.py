from rose import initialize_logging
from rose_vfs.virtualfs import mount_virtualfs, unmount_virtualfs

__all__ = [
    "mount_virtualfs",
    "unmount_virtualfs",
]

initialize_logging(__name__)
