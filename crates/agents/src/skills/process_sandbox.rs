//! Process Sandbox
//!
//! Provides Linux-specific sandboxing for subprocess execution using
//! namespaces, resource limits, and privilege dropping.

use std::path::Path;

/// Apply sandbox constraints to a `tokio::process::Command` before spawning.
/// On non-Linux platforms this is a no-op.
pub fn apply_sandbox(command: &mut tokio::process::Command, _skill_dir: &Path) {
    #[cfg(target_os = "linux")]
    linux::apply(command);

    #[cfg(not(target_os = "linux"))]
    {
        let _ = command;
        let _ = _skill_dir;
    }
}

#[cfg(target_os = "linux")]
mod linux {
    /// Write an error message to stderr using only async-signal-safe functions.
    /// This is safe to call inside `pre_exec`.
    unsafe fn stderr_write(msg: &str) {
        let _ = libc::write(2, msg.as_ptr().cast(), msg.len());
    }

    /// Abort the child process safely from `pre_exec`.
    unsafe fn abort(msg: &str) {
        stderr_write(msg);
        libc::_exit(127);
    }

    pub fn apply(command: &mut tokio::process::Command) {
        unsafe {
            command.pre_exec(|| {
                // 1. Enter new namespaces for isolation
                // CLONE_NEWNS  = mount namespace (file system view)
                // CLONE_NEWPID = PID namespace (process isolation)
                // CLONE_NEWNET = network namespace (no network by default)
                // CLONE_NEWIPC = IPC namespace (isolate sysv ipc / posix mq)
                // CLONE_NEWUTS = UTS namespace (isolate hostname)
                let flags = libc::CLONE_NEWNS
                    | libc::CLONE_NEWPID
                    | libc::CLONE_NEWNET
                    | libc::CLONE_NEWIPC
                    | libc::CLONE_NEWUTS;
                if libc::unshare(flags) != 0 {
                    // Non-fatal: continue even if namespaces fail (e.g. inside Docker without privileges)
                    stderr_write("beebotos-sandbox: unshare failed, continuing without namespaces\n");
                }

                // 2. Prevent privilege escalation (required for seccomp, but we use it even without)
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    abort("beebotos-sandbox: prctl(PR_SET_NO_NEW_PRIVS) failed\n");
                }

                // 3. Drop privileges if running as root and BEE_DROP_PRIVS is set.
                // In dev/test environments (Docker, CI) we often run as root but
                // still need filesystem access, so privilege drop is opt-in.
                let uid = libc::getuid();
                if uid == 0 && std::env::var("BEE_DROP_PRIVS").is_ok() {
                    // Try to switch to 'nobody' (65534)
                    if libc::setgid(65534) != 0 {
                        abort("beebotos-sandbox: setgid(65534) failed\n");
                    }
                    if libc::setuid(65534) != 0 {
                        abort("beebotos-sandbox: setuid(65534) failed\n");
                    }
                }

                // 4. Set resource limits
                // Memory: 512 MB max (virtual address space)
                let mut rlim = libc::rlimit {
                    rlim_cur: 512 * 1024 * 1024,
                    rlim_max: 512 * 1024 * 1024,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                    abort("beebotos-sandbox: setrlimit(RLIMIT_AS) failed\n");
                }

                // CPU: 60 seconds max
                rlim.rlim_cur = 60;
                rlim.rlim_max = 60;
                if libc::setrlimit(libc::RLIMIT_CPU, &rlim) != 0 {
                    abort("beebotos-sandbox: setrlimit(RLIMIT_CPU) failed\n");
                }

                // Number of processes: 32 max
                rlim.rlim_cur = 32;
                rlim.rlim_max = 32;
                if libc::setrlimit(libc::RLIMIT_NPROC, &rlim) != 0 {
                    abort("beebotos-sandbox: setrlimit(RLIMIT_NPROC) failed\n");
                }

                // Open files: 64 max
                rlim.rlim_cur = 64;
                rlim.rlim_max = 64;
                if libc::setrlimit(libc::RLIMIT_NOFILE, &rlim) != 0 {
                    abort("beebotos-sandbox: setrlimit(RLIMIT_NOFILE) failed\n");
                }

                Ok(())
            });
        }
    }
}
