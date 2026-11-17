"""This process's peak memory: the macOS footprint, the Windows working set."""
from __future__ import annotations

import ctypes
import sys


def _windows_peak(handle) -> int:
    """PeakWorkingSetSize of a process handle (this process when None).

    HANDLE types are declared explicitly: ctypes' default int return type
    truncates the 64-bit pseudo-handle and the call fails.
    """
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
            (n, ctypes.c_size_t) for n in (
                "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
                "PagefileUsage", "PeakPagefileUsage")]
    kernel32 = ctypes.WinDLL("kernel32")
    psapi = ctypes.WinDLL("psapi")
    kernel32.GetCurrentProcess.restype = wintypes.HANDLE
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
    c = Counters()
    c.cb = ctypes.sizeof(c)
    h = kernel32.GetCurrentProcess() if handle is None else wintypes.HANDLE(handle)
    if psapi.GetProcessMemoryInfo(h, ctypes.byref(c), c.cb):
        return int(c.PeakWorkingSetSize)
    return 0


def peak_bytes() -> int:
    if sys.platform == "darwin":
        # proc_pid_rusage(RUSAGE_INFO_V4): ri_lifetime_max_phys_footprint is the
        # "peak memory footprint" /usr/bin/time -l prints.
        class RusageInfoV4(ctypes.Structure):
            _fields_ = [("ri_uuid", ctypes.c_uint8 * 16)] + [
                (name, ctypes.c_uint64) for name in (
                    "ri_user_time", "ri_system_time", "ri_pkg_idle_wkups", "ri_interrupt_wkups",
                    "ri_pageins", "ri_wired_size", "ri_resident_size", "ri_phys_footprint",
                    "ri_proc_start_abstime", "ri_proc_exit_abstime", "ri_child_user_time",
                    "ri_child_system_time", "ri_child_pkg_idle_wkups", "ri_child_interrupt_wkups",
                    "ri_child_pageins", "ri_child_elapsed_abstime", "ri_diskio_bytesread",
                    "ri_diskio_byteswritten", "ri_cpu_time_qos_default", "ri_cpu_time_qos_maintenance",
                    "ri_cpu_time_qos_background", "ri_cpu_time_qos_utility", "ri_cpu_time_qos_legacy",
                    "ri_cpu_time_qos_user_initiated", "ri_cpu_time_qos_user_interactive",
                    "ri_billed_system_time", "ri_serviced_system_time", "ri_logical_writes",
                    "ri_lifetime_max_phys_footprint", "ri_instructions", "ri_cycles",
                    "ri_billed_energy", "ri_serviced_energy", "ri_interval_max_phys_footprint",
                    "ri_runnable_time", "ri_flags")]
        info = RusageInfoV4()
        libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
        import os
        if libc.proc_pid_rusage(os.getpid(), 4, ctypes.byref(info)) == 0:
            return int(info.ri_lifetime_max_phys_footprint)
    if sys.platform == "win32":
        peak = _windows_peak(None)
        if peak:
            return peak
    import resource
    return int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss) * 1024


def run_child(args: list[str], **kwargs) -> tuple["subprocess.CompletedProcess", int]:
    """Runs a native host and returns its result and peak memory: the peak
    working set on Windows, the maximum resident set elsewhere (getrusage
    over finished children, so run one host at a time)."""
    import subprocess
    if sys.platform == "win32":
        proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                encoding="utf-8")
        out, err = proc.communicate()
        peak = _windows_peak(int(proc._handle))
        result = subprocess.CompletedProcess(args, proc.returncode, out, err)
        result.check_returncode()
        return result, peak
    import resource
    before = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", check=True, **kwargs)
    after = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    scale = 1 if sys.platform == "darwin" else 1024  # bytes on macOS, KiB on Linux
    return result, (after if after > before else 0) * scale
