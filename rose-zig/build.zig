const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const libroseffi = b.addSharedLibrary(.{
        .name = "rose",
        .root_source_file = .{ .path = "src/ffi.zig" },
        .target = target,
        .optimize = optimize,
    });
    b.installArtifact(libroseffi);
}
