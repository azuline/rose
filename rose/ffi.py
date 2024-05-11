from cffi import FFI

ffi = FFI()

lib = ffi.dlopen("rose")

ffi.cdef("""
    void free_str(void *str);

    char *getRelease();
""")


def get_release() -> str:
    return ffi.string(ffi.gc(lib.getRelease(), lib.free_str)).decode("utf-8")  # type: ignore
