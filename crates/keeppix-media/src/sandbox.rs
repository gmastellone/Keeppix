use std::ffi::OsStr;
use std::process::{Command, Output};

/// Runs a binary in a child process with `rlimit` on Unix. A panic/abort in
/// the child does not kill the parent. No network: the child doesn't
/// inherit any open working sockets (ffmpeg/ffprobe don't open any).
///
/// No seccomp is applied on Linux: that would add an extra C dependency;
/// `rlimit` is the current ceiling. Possible upgrade: `libseccomp` in the
/// child.
///
/// # Errors
/// If the process fails to start.
pub fn run(
    program: impl AsRef<OsStr>,
    args: &[impl AsRef<OsStr>],
    memory_bytes: u64,
    cpu_secs: u64,
) -> std::io::Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(unix)]
    {
        apply_rlimits(&mut cmd, memory_bytes, cpu_secs);
    }
    #[cfg(not(unix))]
    {
        let _ = (memory_bytes, cpu_secs);
    }
    cmd.output()
}

#[cfg(unix)]
fn apply_rlimits(cmd: &mut Command, memory_bytes: u64, cpu_secs: u64) {
    use std::os::unix::process::CommandExt as _;
    unsafe {
        // SAFETY: `pre_exec` runs in the child after `fork`, before `exec`.
        // `setrlimit` is async-signal-safe. We only touch the child's limits.
        cmd.pre_exec(move || {
            let mem = libc::rlimit {
                rlim_cur: memory_bytes,
                rlim_max: memory_bytes,
            };
            let cpu = libc::rlimit {
                rlim_cur: cpu_secs,
                rlim_max: cpu_secs,
            };
            let _ = libc::setrlimit(libc::RLIMIT_AS, &raw const mem);
            let _ = libc::setrlimit(libc::RLIMIT_CPU, &raw const cpu);
            Ok(())
        });
    }
}
