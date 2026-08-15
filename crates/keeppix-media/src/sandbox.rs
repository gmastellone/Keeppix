use std::ffi::OsStr;
use std::process::{Command, Output};

/// Esegue un binario in un processo figlio con `rlimit` su Unix.
/// Un panic/abort nel figlio non uccide il padre. Niente rete: il figlio
/// non eredita socket aperti di lavoro (ffmpeg/ffprobe non ne aprono).
///
/// Su Linux non si applica seccomp in 1b: eviterebbe una dipendenza C
/// extra; `rlimit` è il tetto. Upgrade: `libseccomp` nel figlio.
///
/// # Errors
/// Se il processo non parte.
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
