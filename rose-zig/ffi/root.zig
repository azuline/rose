const std = @import("std");
const rose = @import("rose");

var gpa = std.heap.GeneralPurposeAllocator(.{}){};
const allocator = gpa.allocator();

export fn getRelease() [*:0]const u8 {
    const message = rose.getRelease(allocator) catch |err| switch (err) {
        error.OutOfMemory => @panic("Out of memory"),
    };
    return message.ptr;
}

export fn free_str(str: [*:0]const u8) void {
    const len = std.mem.len(str);
    allocator.free(str[0 .. len + 1]);
}
