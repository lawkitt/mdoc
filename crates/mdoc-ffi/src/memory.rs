//! The process's memory footprint as iOS counts it against the app's limit
//! (`task_vm_info.phys_footprint`, the figure Xcode's memory gauge shows); on
//! Android and Linux, the resident set and its high-water mark
//! (`/proc/self/status` `VmRSS` and `VmHWM`), which ADR 0089 measures the
//! detector budget against.

/// Bytes; zero where the platform has no footprint ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, uniffi::Record)]
pub struct Memory {
    pub footprint_bytes: u64,
    /// The highest footprint since the process started.
    pub peak_footprint_bytes: u64,
}

#[uniffi::export]
pub fn memory() -> Memory {
    imp::memory()
}

#[cfg(target_vendor = "apple")]
mod imp {
    use super::Memory;

    // <mach/task_info.h>: TASK_VM_INFO and its 4-byte packed layout.
    const TASK_VM_INFO: i32 = 22;
    const TASK_VM_INFO_COUNT: u32 = 93;
    const PHYS_FOOTPRINT: usize = 144;
    const LEDGER_PHYS_FOOTPRINT_PEAK: usize = 168;

    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: i32, info: *mut u32, count: *mut u32) -> i32;
    }

    pub fn memory() -> Memory {
        let mut info = [0u32; TASK_VM_INFO_COUNT as usize];
        let mut count = TASK_VM_INFO_COUNT;
        // SAFETY: `info` holds `count` naturals, as task_info requires, and
        // mach_task_self_ is the task's own send right, set by the loader.
        let status =
            unsafe { task_info(mach_task_self_, TASK_VM_INFO, info.as_mut_ptr(), &mut count) };
        if status != 0 {
            return Memory::default();
        }
        let word = |offset: usize| {
            let low = u64::from(info[offset / 4]);
            let high = u64::from(info[offset / 4 + 1]);
            low | high << 32
        };
        Memory {
            footprint_bytes: word(PHYS_FOOTPRINT),
            // Revision 3 and later; older kernels leave it zero.
            peak_footprint_bytes: if count as usize > LEDGER_PHYS_FOOTPRINT_PEAK / 4 + 1 {
                word(LEDGER_PHYS_FOOTPRINT_PEAK)
            } else {
                0
            },
        }
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
mod imp {
    pub fn memory() -> super::Memory {
        std::fs::read_to_string("/proc/self/status")
            .map(|status| super::parse_proc_status(&status))
            .unwrap_or_default()
    }
}

/// `VmRSS` and `VmHWM` from a `/proc/<pid>/status`, given in kB. Tested on
/// every platform; read only on Android and Linux.
#[cfg_attr(not(any(target_os = "android", target_os = "linux")), allow(dead_code))]
fn parse_proc_status(status: &str) -> Memory {
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|value| value.trim().strip_suffix("kB"))
            .and_then(|kb| kb.trim().parse::<u64>().ok())
            .map_or(0, |kb| kb * 1024)
    };
    Memory {
        footprint_bytes: field("VmRSS"),
        peak_footprint_bytes: field("VmHWM"),
    }
}

#[cfg(not(any(target_vendor = "apple", target_os = "android", target_os = "linux")))]
mod imp {
    pub fn memory() -> super::Memory {
        super::Memory::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_resident_set_and_its_peak() {
        let status = "Name:\tmdoc\nVmHWM:\t  204800 kB\nVmRSS:\t  102400 kB\nThreads:\t4\n";
        let memory = super::parse_proc_status(status);
        assert_eq!(memory.footprint_bytes, 100 * 1024 * 1024);
        assert_eq!(memory.peak_footprint_bytes, 200 * 1024 * 1024);
        let empty = super::parse_proc_status("Name:\tmdoc\n");
        assert_eq!(empty, super::Memory::default());
    }

    #[test]
    #[cfg(target_vendor = "apple")]
    fn footprint_grows_with_touched_memory() {
        let before = super::memory();
        assert!(before.footprint_bytes > 0);
        assert!(before.peak_footprint_bytes >= before.footprint_bytes);
        let block = vec![1u8; 64 * 1024 * 1024];
        let during = super::memory();
        assert!(during.footprint_bytes >= before.footprint_bytes + 32 * 1024 * 1024);
        assert!(during.peak_footprint_bytes >= during.footprint_bytes);
        drop(std::hint::black_box(block));
    }
}
