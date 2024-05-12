const std = @import("std");
const sqlite = @import("sqlite");
const testing = std.testing;

pub fn getRelease(allocator: std.mem.Allocator) ![:0]const u8 {
    return try allocator.dupeZ(u8, "hello");
}

pub fn getTrack(allocator: std.mem.Allocator) void {
    const db = try sqlite.Db.init(.{
        .mode = sqlite.Db.Mode{ .File = "/home/blissful/.cache/rose/cache.sqlite3" },
        .open_flags = .{
            .write = true,
            .create = true,
        },
        .threading_mode = .MultiThread,
    });
    _ = db;
    _ = allocator;
}

test "basic add functionality" {
    const message = try getRelease(testing.allocator);
    try testing.expect(std.mem.eql(u8, message, "hello"));
    testing.allocator.free(message);
}
