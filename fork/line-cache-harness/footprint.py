#!/usr/bin/env python3
"""Prints one process's physical memory footprint in bytes (macOS).

Usage: footprint.py <pid>

The footprint is the figure Activity Monitor shows as Memory: it counts
compressed and swapped pages, which resident size leaves out.
"""
import ctypes
import sys


class RusageInfoV0(ctypes.Structure):
    _fields_ = [
        ("uuid", ctypes.c_uint8 * 16),
        ("user_time", ctypes.c_uint64),
        ("system_time", ctypes.c_uint64),
        ("pkg_idle_wkups", ctypes.c_uint64),
        ("interrupt_wkups", ctypes.c_uint64),
        ("pageins", ctypes.c_uint64),
        ("wired_size", ctypes.c_uint64),
        ("resident_size", ctypes.c_uint64),
        ("phys_footprint", ctypes.c_uint64),
        ("proc_start_abstime", ctypes.c_uint64),
        ("proc_exit_abstime", ctypes.c_uint64),
    ]


libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
libproc.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
usage = RusageInfoV0()
if libproc.proc_pid_rusage(int(sys.argv[1]), 0, ctypes.byref(usage)) != 0:  # RUSAGE_INFO_V0
    sys.exit("proc_pid_rusage failed: errno %d" % ctypes.get_errno())
print(usage.phys_footprint)
