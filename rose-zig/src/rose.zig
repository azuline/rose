const std = @import("std");

pub fn getRelease(allocator: std.mem.Allocator) ![:0]const u8 {
    return try allocator.dupeZ(u8, "hello");
}
