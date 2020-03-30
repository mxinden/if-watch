use std::{
    io::{Error, ErrorKind, Result},
    os::unix::io::{AsRawFd, RawFd},
};

#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
struct rt_msghdr {
    rtm_msglen: u16,
    rtm_version: u8,
    rtm_type: u8,
    rtm_hdrlen: u16,
    rtm_index: u16,
    rtm_tableid: u16,
    rtm_priority: u8,
    rtm_mpls: u8,
    rtm_addrs: std::os::raw::c_int,
    rtm_flags: std::os::raw::c_int,
    rtm_fmask: std::os::raw::c_int,
    rtm_pid: libc::pid_t,
    rtm_seq: std::os::raw::c_int,
    rtm_errno: std::os::raw::c_int,
    rtm_inits: std::os::raw::c_uint,
    rtm_rmx: rt_metrics,
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
struct rt_metrics {
    rmx_pkgsent: u64,
    rmx_expire: i64,
    rmx_locks: std::os::raw::c_uint,
    rmx_mtu: std::os::raw::c_uint,
    rmx_refcmt: std::os::raw::c_uint,
    rmx_hopcount: std::os::raw::c_uint,
    rmx_recvpipe: std::os::raw::c_uint,
    rmx_sendpipe: std::os::raw::c_uint,
    rmx_ssthresh: std::os::raw::c_uint,
    rmx_rtt: std::os::raw::c_uint,
    rmx_rttvar: std::os::raw::c_uint,
    rmx_pad: std::os::raw::c_uint,
}
