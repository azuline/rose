const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const rose = b.dependency("rose", .{
        .target = target,
        .optimize = optimize,
    });

    const librose = b.addSharedLibrary(.{
        .name = "rose",
        .root_source_file = .{ .path = "src/root.zig" },
        .target = target,
        .optimize = optimize,
    });
    // TODO: Doesn't work. How do I depend?
    librose.linkLibrary(rose.artifact("rose"));
    b.installArtifact(librose);
}
