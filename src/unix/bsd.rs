use std::{
    io::{Error, ErrorKind, Result},
    os::unix::io::{AsRawFd, RawFd},
    os::raw::{c_int, c_uint, c_short, c_ushort, c_ulong},
};

use super::aligned_buffer::{FromBuffer, U32AlignedBuffer as U64AlignedBuffer};

#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
#[allow(non_camel_case_types)]
#[cfg(target_os = "openbsd")]
struct rt_msghdr {
    rtm_msglen: u16, // 2
    rtm_version: u8, // 3
    rtm_type: u8, // 4
    rtm_hdrlen: u16, // 6
    rtm_index: u16, // 8
    rtm_tableid: u16, // 10
    rtm_priority: u8, // 11
    rtm_mpls: u8, // 12
    rtm_addrs: i32, // 16
    rtm_flags: i32, // 20
    rtm_fmask: i32, // 24
    rtm_pid: i32, // 28
    rtm_seq: i32, // 28
    rtm_errno: i32, // 32
    rtm_inits: u32, // 36
    _rtm_pad: [u8; 4], // 40
    rtm_rmx: rt_metrics,
}
#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
#[allow(non_camel_case_types)]
#[cfg(target_os = "freebsd")]
struct rt_msghdr {
    rtm_msglen: c_ushort, // 2
    rtm_version: c_uchar, // 3
    rtm_type: c_uchar, // 4
    rtm_index: c_ushort, // 6
    _rtm_pad: [u8; 2], // 8
    rtm_flags: c_int, // 12
    rtm_addrs: c_int, // 16
    rtm_pid: libc::pid_t, // 20
    rtm_seq: c_int, // 24
    rtm_errno: c_int, // 28
    rtm_fmask: c_int, // 32
    rtm_inits: std::os::raw::c_ulong, // 36 or 40
    rtm_rmx: rt_metrics,
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
#[allow(non_camel_case_types)]
#[cfg(target_os = "openbsd")]
struct rt_metrics {
    rmx_pkgsent: u64, // 48
    rmx_expire: i64, // 56
    rmx_locks: u32, // 60
    rmx_mtu: u32, // 64
    rmx_refcmt: u32, // 68
    rmx_hopcount: u32, // 72
    rmx_recvpipe: u32, // 76
    rmx_sendpipe: u32, // 80
    rmx_ssthresh: u32, // 84
    rmx_rtt: u32, // 88
    rmx_rttvar: u32, // 92
    rmx_pad: u32, // 96
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
#[allow(non_camel_case_types)]
#[cfg(target_os = "freebsd")]
struct rt_metrics {
    rmx_locks: c_ulong, // 40/48
    rmx_mtu: c_ulong, // 44/56
    rmx_hopcount: c_ulong, // 48/64
    rmx_expire: c_ulong, // 52/72
    rmx_recvpipe: c_ulong, // 56/80
    rmx_sendpipe: c_ulong, // 60/88
    rmx_ssthresh: c_ulong, // 64/96
    rmx_rtt: c_ulong, // 68/104
    rmx_rttvar: c_ulong, // 72/112
    rmx_pksent: c_ulong, // 76/120
    rmx_weight: c_ulong, // 80/128
    rmx_filler: [c_ulong; 3], // 92/152
}

unsafe impl FromBuffer for rt_msghdr {
    fn len(&self, _: usize) -> u32 {
        if false {
            let _: [u8; algin_of!(rt_msghdr) - 8] = [];
            let _: [u8; size_of!(std::os::raw::c_short)] = [0u8; 2];
            let _: [u8; size_of!(std::os::raw::c_ushort)] = [0u8; 2];
            let _: [u8; size_of!(std::os::raw::c_uint)] = [0u8; 4];
            let _: [u8; size_of!(libc::pid_t)] = [0u8; 4];
            let _: [u8; size_of!(rt_msghdr)] = [0u8; 96];
            let _: [u8; size_of!(rt_metrics)] = [0u8; 56];
        }
        self.rtm_msglen as _
    }
}

const RTM_VERSION: u8 = 5;

struct Routes<'a> {
    buffer: U64AlignedBuffer<'a>,
}

impl<'a> Iterator for Routes<'a> {
    type Item = ();
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let hdr: rt_msghdr = self.buffer.read()?;
            if hdr.rtm_version != RTM_VERSION {
                continue;
            }
        }
    }
}
