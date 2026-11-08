//! Sub-command dispatch: one `match` from `Commands` to the command
//! implementation that runs it.

use super::*;

pub(crate) async fn execute_command(
    cmd: Commands,
    global_binds: Vec<String>,
    engine: EngineChoice,
) -> Result<()> {
    pn_trace!("DEBUG execute_command: START {:?}", cmd);
    match cmd {
        // Container commands
        Commands::Install {
            image,
            name,
            force,
            checksum,
        } => cmd_install(image, name, force, checksum).await,
        Commands::List {
            all,
            no_all,
            format,
        } => cmd_list(all, no_all, format).await,
        Commands::Login {
            container,
            user,
            isolated,
            command,
        } => cmd_login(container, user, isolated, command, global_binds, engine).await,
        Commands::Run {
            container,
            command,
            rm,
            detach,
            name,
            network,
            volume,
            publish,
            entrypoint,
            tty,
            interactive,
            user,
            workdir,
            isolated,
            ..   // the global seccomp/trace flags are handled where the engine is configured
        } => {
            cmd_run(
                container,
                command,
                rm,
                detach,
                name,
                network,
                volume,
                publish,
                entrypoint,
                tty,
                interactive,
                user,
                workdir,
                isolated,
                global_binds,
                engine,
            )
            .await
        }
        Commands::Ps { format } => cmd_ps(format).await,
        Commands::Sessions { format } => cmd_sessions(format).await,
        Commands::Attach {
            container,
            user,
            command,
        } => cmd_attach(container, user, command).await,
        Commands::Kill { container, signal } => cmd_kill(container, signal).await,
        Commands::Remove { name, force, yes } => cmd_remove(name, force || yes).await,
        Commands::Rename { oldname, newname } => cmd_rename(oldname, newname).await,
        Commands::Copy {
            source,
            destination,
            recursive,
            move_,
        } => cmd_copy(source, destination, recursive, move_).await,
        Commands::Reset { name } => cmd_reset(name).await,
        Commands::Sync {
            source,
            destination,
            delete,
        } => cmd_sync(source, destination, delete).await,
        Commands::Search {
            query,
            limit,
            format,
        } => cmd_search(query, limit, format).await,
        Commands::Image(cmd) => execute_image_command(cmd).await,
        Commands::Network(cmd) => execute_network_command(cmd).await,
        Commands::Volume(cmd) => execute_volume_command(cmd).await,
        Commands::Info { format } => cmd_info(format).await,
        Commands::Version { format } => cmd_version(format).await,
        Commands::Doctor { container, format } => cmd_doctor(container, format).await,

        // Service orchestration commands
        Commands::Service(cmd) => execute_service_command(cmd).await,
        Commands::Daemon(cmd) => execute_daemon_command(cmd).await,
        Commands::Watchdog(cmd) => execute_watchdog_command(cmd).await,
        Commands::Quota(cmd) => execute_quota_command(cmd).await,
        Commands::Log(cmd) => execute_log_command(cmd).await,
        Commands::Dbus(cmd) => execute_dbus_command(cmd).await,
        Commands::Socket(cmd) => execute_socket_command(cmd).await,
        Commands::Boot(cmd) => execute_boot_command(cmd).await,
        Commands::Wakelock(cmd) => execute_wakelock_command(cmd).await,
        Commands::Policy(cmd) => execute_policy_command(cmd).await,

        // Phase 7: Platform Maturity commands
        Commands::Backup(cmd) => execute_backup_command(cmd).await,
        Commands::Build(cmd) => execute_build_command(cmd).await,
        Commands::Config(cmd) => execute_config_command(cmd),
        Commands::Gitops(cmd) => execute_gitops_command(cmd).await,
        Commands::Dashboard(cmd) => execute_dashboard_command(cmd).await,
        Commands::Docker { container, args } => cmd_docker(container, args).await,
        Commands::Compose {
            files,
            image,
            output,
        } => execute_compose_command(files, image, output).await,

        // Utilities
        Commands::Completion { shell } => cmd_completion(shell).await,
    }
}
