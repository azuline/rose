import fuse

from rose.foundation.conf import Config

fuse.fuse_python_api = (0, 2)


class VirtualFS(fuse.Fuse):
    pass


def start_virtualfs(c: Config):
    server = VirtualFS()
    server.parse([str(c.fuse_mount_dir)])
    server.main()
