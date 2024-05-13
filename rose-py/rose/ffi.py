import os

from cffi import FFI

from rose.common import RoseError

try:
    so_path = os.environ["ROSE_SO_PATH"]
except KeyError as e:
    raise RoseError("ROSE_SO_PATH unset: cannot load underlying Zig library") from e

ffi = FFI()
lib = ffi.dlopen(so_path)
ffi.cdef("""
    void free_str(void *str);

    char *getRelease();
""")


def get_release() -> str:
    return ffi.string(ffi.gc(lib.getRelease(), lib.free_str)).decode("utf-8")  # type: ignore
