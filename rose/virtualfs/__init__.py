import errno
import os
import stat
import subprocess

import fuse

from rose.foundation.conf import Config

fuse.fuse_python_api = (0, 2)


class VirtualFS(fuse.Fuse):  # type: ignore
    def getattr(self, path: str) -> fuse.Stat | int:
        if path[1:] == "some_dir" or path in ["..", "/"]:
            st_mode = stat.S_IFDIR | 0o755
        elif path[1:] == "some_file":
            st_mode = stat.S_IFREG | 0o644
        else:
            return -errno.ENOENT

        return fuse.Stat(
            st_nlink=1,
            st_mode=st_mode,
            st_uid=os.getuid(),
            st_gid=os.getgid(),
        )

    def readdir(self, path: str, _):
        if path == "/":
            for name in [".", "..", "some_file", "some_dir"]:
                yield fuse.Direntry(name)


def mount_virtualfs(c: Config) -> None:
    server = VirtualFS()
    server.parse([str(c.fuse_mount_dir)])
    server.main()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
