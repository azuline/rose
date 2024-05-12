const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const sqlite = b.dependency("sqlite", .{
        .target = target,
        .optimize = optimize,
    });

    const rose = b.addModule("rose", .{
        .root_source_file = b.path("rose/root.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &[_]std.Build.Module.Import{
            .{ .name = "sqlite", .module = sqlite.module("sqlite") },
        },
    });

    const librose = b.addSharedLibrary(.{
        .name = "rose",
        .root_source_file = .{ .path = "ffi/root.zig" },
        .target = target,
        .optimize = optimize,
    });
    librose.root_module.addImport("rose", rose);
    b.installArtifact(librose);
}
