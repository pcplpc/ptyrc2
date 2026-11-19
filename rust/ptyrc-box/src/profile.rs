// SPDX-License-Identifier: GPL-3.0-or-later
//! The OS profile of the runner (plan, Model: the third wall). Applied
//! once everything that needs a file is done and the socket is open:
//! a hole in the engine is then not the account.
//!
//! Linux, to every thread of the process:
//!
//! - Landlock: every access right the kernel knows is handled, no rule
//!   is added: no file is opened, created or executed; ABI 4 and on, no
//!   TCP bind or connect; ABI 6 and on, no abstract unix socket and no
//!   signal outside the process. ABI 8 and on, every thread at once;
//!   before, the calling thread only (seccomp covers the others).
//! - `PR_SET_NO_NEW_PRIVS`.
//! - seccomp, two filters. The first kills the process on a call no
//!   path of the runner makes and an attack would (`KILL`). The second
//!   allows what the running engine needs (`ALLOW`) and answers `EPERM`
//!   to the rest: files, sockets, signals to others; std and the engine
//!   may probe some of these on a cold path (a panic, a cgroup), and a
//!   probe must fail, not end the app.
//!
//! Best effort: what the kernel lacks is named in `Status::missing`.
//! Elsewhere than Linux: nothing is applied, and that is said.

/// What was applied, and what was not.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// What was applied, a line each.
    pub applied: Vec<String>,
    /// What the full profile has and this one has not, a line each.
    pub missing: Vec<String>,
}

impl Status {
    /// Nothing missing.
    pub fn full(&self) -> bool {
        self.missing.is_empty()
    }

    /// Lines for the log.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .applied
            .iter()
            .map(|l| format!("profile: {l}"))
            .collect();
        out.extend(
            self.missing
                .iter()
                .map(|l| format!("profile: MISSING: {l}")),
        );
        out
    }
}

/// Apply the profile to the process. Not undone.
pub fn apply() -> Status {
    let mut status = Status::default();
    imp::apply(&mut status);
    status
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub fn apply(status: &mut super::Status) {
        status
            .missing
            .push(format!("no OS profile on {}", std::env::consts::OS));
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Status;
    use landlock::{
        ABI, Access, AccessFs, AccessNet, LandlockStatus, RestrictSelfAttr, Ruleset, RulesetAttr,
        RulesetStatus, Scope,
    };
    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen as Len, SeccompCmpOp as Op, SeccompCondition,
        SeccompFilter, SeccompRule, TargetArch,
    };
    use std::collections::BTreeMap;

    /// The newest ABI this build knows.
    const LATEST: ABI = ABI::V9;

    pub fn apply(status: &mut Status) {
        landlock(status);
        seccomp(status);
    }

    fn landlock(status: &mut Status) {
        let done = Ruleset::default()
            .handle_access(AccessFs::from_all(LATEST))
            .and_then(|r| r.handle_access(AccessNet::from_all(LATEST)))
            .and_then(|r| r.scope(Scope::from_all(LATEST)))
            .and_then(|r| r.create())
            .and_then(|r| r.all_threads(true))
            .and_then(|r| r.restrict_self());
        let done = match done {
            Ok(done) => done,
            Err(e) => {
                status.missing.push(format!("landlock: {e}"));
                return;
            }
        };
        let (abi, kernel) = match done.landlock {
            LandlockStatus::Available {
                effective_abi,
                kernel_abi,
            } => (effective_abi, kernel_abi),
            LandlockStatus::NotEnabled => {
                status.missing.push("landlock: not enabled at boot".into());
                return;
            }
            LandlockStatus::NotImplemented => {
                status.missing.push("landlock: not in this kernel".into());
                return;
            }
        };
        if done.ruleset == RulesetStatus::NotEnforced {
            status.missing.push("landlock: not enforced".into());
            return;
        }
        let mut has = vec!["files"];
        let mut lacks = Vec::new();
        for (since, what) in [
            (ABI::V2, "renames across directories (refer)"),
            (ABI::V3, "truncate"),
            (ABI::V4, "TCP bind and connect"),
            (ABI::V5, "ioctl on devices"),
            (ABI::V6, "abstract unix sockets, signals"),
        ] {
            if abi >= since {
                has.push(what);
            } else {
                lacks.push(what);
            }
        }
        let threads = if done.all_threads {
            "every thread"
        } else {
            lacks.push("every thread (ABI 8): the calling thread only");
            "the calling thread"
        };
        // Rights of a newer kernel are unknown here: not handled.
        let newer = kernel.map_or(String::new(), |k| format!(" (the kernel's: {k})"));
        status.applied.push(format!(
            "landlock ABI {}{newer}: {}; {threads}",
            abi as i32,
            has.join(", ")
        ));
        for what in lacks {
            status
                .missing
                .push(format!("landlock ABI {}: {what}", abi as i32));
        }
    }

    fn arch() -> Option<TargetArch> {
        match std::env::consts::ARCH {
            "x86_64" => Some(TargetArch::x86_64),
            "aarch64" => Some(TargetArch::aarch64),
            "riscv64" => Some(TargetArch::riscv64),
            _ => None,
        }
    }

    /// No path of the runner makes these once the profile is applied;
    /// an attack would: the process is killed.
    fn killed() -> Vec<i64> {
        let mut v = vec![
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_process_madvise,
            libc::SYS_pidfd_getfd,
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
            libc::SYS_bpf,
            libc::SYS_perf_event_open,
            libc::SYS_userfaultfd,
            libc::SYS_keyctl,
            libc::SYS_add_key,
            libc::SYS_request_key,
            libc::SYS_unshare,
            libc::SYS_setns,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_chroot,
            libc::SYS_open_by_handle_at,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_kexec_load,
            libc::SYS_reboot,
        ];
        #[cfg(target_arch = "x86_64")]
        v.extend([libc::SYS_fork, libc::SYS_vfork, libc::SYS_kexec_file_load]);
        v
    }

    /// What the running engine needs: the socket and stderr (open
    /// already), memory (guest growth, the JIT), threads (the epoch,
    /// the compiler's pool), signals to itself (traps), time.
    fn allowed() -> Vec<i64> {
        vec![
            libc::SYS_read,
            libc::SYS_readv,
            libc::SYS_write,
            libc::SYS_writev,
            libc::SYS_recvfrom,
            libc::SYS_recvmsg,
            libc::SYS_sendto,
            libc::SYS_sendmsg,
            libc::SYS_close,
            libc::SYS_futex,
            libc::SYS_mmap,
            libc::SYS_munmap,
            libc::SYS_mprotect,
            libc::SYS_mremap,
            libc::SYS_madvise,
            libc::SYS_brk,
            libc::SYS_membarrier,
            libc::SYS_clock_gettime,
            libc::SYS_clock_getres,
            libc::SYS_gettimeofday,
            libc::SYS_clock_nanosleep,
            libc::SYS_nanosleep,
            // A sleep stopped (SIGSTOP: the driver's pause) and
            // continued, or interrupted by a signal handled.
            libc::SYS_restart_syscall,
            libc::SYS_sched_yield,
            libc::SYS_sched_getaffinity,
            libc::SYS_sigaltstack,
            libc::SYS_rt_sigaction,
            libc::SYS_rt_sigprocmask,
            libc::SYS_rt_sigreturn,
            libc::SYS_getrandom,
            libc::SYS_getpid,
            libc::SYS_gettid,
            libc::SYS_set_robust_list,
            libc::SYS_rseq,
            libc::SYS_exit,
            libc::SYS_exit_group,
        ]
    }

    fn cond(arg: u8, op: Op, value: u64) -> SeccompCondition {
        SeccompCondition::new(arg, Len::Qword, op, value).expect("an argument of 0..6")
    }

    fn rule(conditions: Vec<SeccompCondition>) -> SeccompRule {
        SeccompRule::new(conditions).expect("a condition")
    }

    /// Allowed under a condition on the arguments.
    fn allowed_if() -> Vec<(i64, Vec<SeccompRule>)> {
        let pid = u64::from(std::process::id());
        let sol = libc::SOL_SOCKET as u64;
        let opt = |o: i32| rule(vec![cond(1, Op::Eq, sol), cond(2, Op::Eq, o as u64)]);
        let thread = libc::CLONE_THREAD as u64;
        vec![
            // A thread, not a process: no namespace, no fork.
            (
                libc::SYS_clone,
                vec![rule(vec![cond(0, Op::MaskedEq(thread), thread)])],
            ),
            // `UnixStream::set_read_timeout`.
            (
                libc::SYS_setsockopt,
                vec![opt(libc::SO_RCVTIMEO), opt(libc::SO_SNDTIMEO)],
            ),
            // Whether a descriptor is open (std, before a close).
            (
                libc::SYS_fcntl,
                vec![rule(vec![cond(1, Op::Eq, libc::F_GETFD as u64)])],
            ),
            // A signal to a thread of this process (`abort`).
            (libc::SYS_tgkill, vec![rule(vec![cond(0, Op::Eq, pid)])]),
            // Thread names.
            (
                libc::SYS_prctl,
                vec![
                    rule(vec![cond(0, Op::Eq, libc::PR_SET_NAME as u64)]),
                    rule(vec![cond(0, Op::Eq, libc::PR_GET_NAME as u64)]),
                ],
            ),
        ]
    }

    fn program(
        rules: BTreeMap<i64, Vec<SeccompRule>>,
        mismatch: SeccompAction,
        matched: SeccompAction,
        arch: TargetArch,
    ) -> Result<BpfProgram, seccompiler::BackendError> {
        SeccompFilter::new(rules, mismatch, matched, arch)?.try_into()
    }

    fn seccomp(status: &mut Status) {
        let Some(arch) = arch() else {
            status
                .missing
                .push(format!("seccomp: no filter for {}", std::env::consts::ARCH));
            return;
        };
        let kill: BTreeMap<_, _> = killed().into_iter().map(|n| (n, vec![])).collect();
        let mut allow: BTreeMap<_, _> = allowed().into_iter().map(|n| (n, vec![])).collect();
        allow.extend(allowed_if());
        // clone3 hides its flags in memory: ENOSYS, and the C library
        // falls back to clone. Allowed by the last filter so that the
        // answer of its own is not EPERM (the last installed wins
        // among errors).
        let enosys = BTreeMap::from([(libc::SYS_clone3, vec![])]);
        allow.insert(libc::SYS_clone3, vec![]);
        let programs = [
            program(kill, SeccompAction::Allow, SeccompAction::KillProcess, arch),
            program(
                enosys,
                SeccompAction::Allow,
                SeccompAction::Errno(libc::ENOSYS as u32),
                arch,
            ),
            program(
                allow,
                SeccompAction::Errno(libc::EPERM as u32),
                SeccompAction::Allow,
                arch,
            ),
        ];
        let mut done = 0;
        for p in programs {
            let applied = p
                .map_err(|e| e.to_string())
                .and_then(|p| seccompiler::apply_filter_all_threads(&p).map_err(|e| e.to_string()));
            match applied {
                Ok(()) => done += 1,
                Err(e) => {
                    status.missing.push(format!("seccomp: {e}"));
                    return;
                }
            }
        }
        debug_assert_eq!(done, 3);
        status.applied.push(format!(
            "seccomp, every thread: {} calls allowed, {} kill the process, the rest EPERM",
            allowed().len() + allowed_if().len(),
            killed().len()
        ));
    }
}
