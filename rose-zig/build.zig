const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Dependencies.
    const sqlite = b.dependency("sqlite", .{
        .target = target,
        .optimize = optimize,
    });
    const ffmpeg = b.dependency("ffmpeg", .{
        .target = target,
        .optimize = optimize,
    });

    // Specify the core library module.
    const rose = b.addModule("rose", .{
        .root_source_file = b.path("rose/root.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &[_]std.Build.Module.Import{
            .{ .name = "av", .module = ffmpeg.module("av") },
            .{ .name = "sqlite", .module = sqlite.module("sqlite") },
        },
    });

    // Tests for the core library module.
    const test_step = b.step("test", "Run unit tests");
    const unit_tests = b.addTest(.{
        .root_source_file = .{ .path = "rose/root.zig" },
        .target = target,
    });
    const run_unit_tests = b.addRunArtifact(unit_tests);
    test_step.dependOn(&run_unit_tests.step);

    // Shared library for compatibility with other languages.
    const librose = b.addSharedLibrary(.{
        .name = "rose",
        .root_source_file = .{ .path = "ffi/root.zig" },
        .target = target,
        .optimize = optimize,
    });
    librose.root_module.addImport("rose", rose);
    b.installArtifact(librose);
}
