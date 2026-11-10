## What is absent

`podnext build local` does not produce an image. `cmd_build_local` in
`crates/podnext/src/commands/mod.rs` parses the Dockerfile and optionally executes `RUN`. `RUN`
support arrived in `2f93f2c` for #17. The command then computes a **layer diff of the raw
context directory** as a tar.gz. It prints the digest of the tar.gz and deletes the tarball. The
command registers nothing as an image.

Thus `podnext build local -t test:latest .` reports `Successfully built image 'test:latest'`.
No image exists to run.

## What a real implementation needs

This work is not a small change. It thus needs a separate issue and not a line in #17:

1. **Pull the base image** that `FROM` names, or resolve it locally, so there is a rootfs to build on.
2. **Assemble a real rootfs.** Apply the context and the filesystem effect of each instruction
   on top of that base. Do not tar the context in isolation.
3. **Produce a runnable result.** Register the result so `podnext run <tag>` works. Store the
   layer and rootfs where the container machinery expects them.
4. **Stop the false success message.** Until steps 1-3 exist, the command must report what it
   does: a digest of a layer diff. It must not report "Successfully built image".

The parser reads `ENV` and `WORKDIR` but the command does not apply them. The reason is the
same: no assembled rootfs exists for the application. They belong in this work.

## Related

- `ARG` is absent at the parser level. `crates/proot-distro-core/src/dockerfile.rs` still has
  no `Arg` variant in `DockerfileInstruction` and `parse_instruction`. `commands/mod.rs` works
  around this gap. The parser is the correct location.

Labels: enhancement
