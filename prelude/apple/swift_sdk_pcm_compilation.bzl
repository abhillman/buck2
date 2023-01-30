# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(":apple_utility.bzl", "expand_relative_prefixed_sdk_path", "get_disable_pch_validation_flags")
load(":swift_toolchain_types.bzl", "SdkCompiledModuleInfo")

def compile_swift_sdk_pcm(
        ctx: "context",
        toolchain_context: struct.type,
        sdk_deps_set: "SDKDepTSet",
        uncompiled_sdk_module_info: "SdkUncompiledModuleInfo",
        sdk_module_providers: {str.type: "SdkCompiledModuleInfo"}):
    module_name = uncompiled_sdk_module_info.module_name

    cmd = cmd_args(toolchain_context.compiler)
    cmd.add(uncompiled_sdk_module_info.partial_cmd)
    cmd.add(["-sdk", toolchain_context.sdk_path])
    cmd.add(toolchain_context.compiler_flags)

    if toolchain_context.swift_resource_dir:
        cmd.add([
            "-resource-dir",
            toolchain_context.swift_resource_dir,
        ])

    cmd.add(sdk_deps_set.project_as_args("clang_deps"))

    expanded_modulemap_path_cmd = expand_relative_prefixed_sdk_path(
        cmd_args(toolchain_context.sdk_path),
        cmd_args(toolchain_context.swift_resource_dir),
        uncompiled_sdk_module_info.input_relative_path,
    )
    pcm_output = ctx.actions.declare_output(module_name + ".pcm")
    cmd.add([
        "-o",
        pcm_output.as_output(),
        expanded_modulemap_path_cmd,
    ])

    # For SDK modules we need to set a few more args
    cmd.add([
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-emit-module",
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-fsystem-module",
    ])

    _add_sdk_module_search_path(cmd, uncompiled_sdk_module_info, toolchain_context)

    sdk_module_providers[uncompiled_sdk_module_info.name] = SdkCompiledModuleInfo(
        name = uncompiled_sdk_module_info.name,
        module_name = module_name,
        is_framework = uncompiled_sdk_module_info.is_framework,
        output_artifact = pcm_output,
        is_swiftmodule = False,
        deps = sdk_deps_set,
        input_relative_path = expanded_modulemap_path_cmd,
    )

    ctx.actions.run(cmd, category = "sdk_swift_pcm_compile", identifier = module_name)

def get_shared_pcm_compilation_args(target: str.type, module_name: str.type) -> "cmd_args":
    cmd = cmd_args()
    cmd.add([
        "-emit-pcm",
        "-target",
        target,
        "-module-name",
        module_name,
        "-Xfrontend",
        "-disable-implicit-swift-modules",
        "-Xcc",
        "-fno-implicit-modules",
        "-Xcc",
        "-fno-implicit-module-maps",
        # Disable debug info in pcm files. This is required to avoid embedding absolute paths
        # and ending up with mismatched pcm file sizes.
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-fmodule-format=raw",
        # Embed all input files into the PCM so we don't need to include module map files when
        # building remotely.
        # https://github.com/apple/llvm-project/commit/fb1e7f7d1aca7bcfc341e9214bda8b554f5ae9b6
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-fmodules-embed-all-files",
        # Embed all files that were read during compilation into the generated PCM.
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-fmodule-file-home-is-cwd",
        # Once we have an empty working directory the compiler provided headers such as float.h
        # cannot be found, so add . to the header search paths.
        "-Xcc",
        "-I.",
    ])

    cmd.add(get_disable_pch_validation_flags())

    return cmd

def _remove_path_components_from_right(path: str.type, count: int.type):
    path_components = path.split("/")
    removed_path = "/".join(path_components[0:-count])
    return removed_path

def _add_sdk_module_search_path(cmd, uncompiled_sdk_module_info, toolchain_context):
    modulemap_path = uncompiled_sdk_module_info.input_relative_path

    # If this input is a framework we need to search above the
    # current framework location, otherwise we include the
    # modulemap root.
    if uncompiled_sdk_module_info.is_framework:
        frameworks_dir_path = _remove_path_components_from_right(modulemap_path, 3)
        expanded_path = expand_relative_prefixed_sdk_path(
            cmd_args(toolchain_context.sdk_path),
            cmd_args(toolchain_context.swift_resource_dir),
            frameworks_dir_path,
        )
    else:
        module_root_path = _remove_path_components_from_right(modulemap_path, 1)
        expanded_path = expand_relative_prefixed_sdk_path(
            cmd_args(toolchain_context.sdk_path),
            cmd_args(toolchain_context.swift_resource_dir),
            module_root_path,
        )

    cmd.add([
        "-Xcc",
        ("-F" if uncompiled_sdk_module_info.is_framework else "-I"),
        "-Xcc",
        cmd_args(expanded_path),
    ])
