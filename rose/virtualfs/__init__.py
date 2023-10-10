import subprocess

import fuse

from rose.foundation.conf import Config

fuse.fuse_python_api = (0, 2)


class VirtualFS(fuse.Fuse):
    pass


def mount_virtualfs(c: Config) -> None:
    server = VirtualFS()
    server.parse([str(c.fuse_mount_dir)])
    server.main()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
