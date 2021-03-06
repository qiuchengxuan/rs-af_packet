extern crate libc;

use libc::{
    c_char, c_int, c_short, c_uint, c_ulong, c_void, getsockopt, if_nametoindex, ioctl, setsockopt,
    socket, socklen_t, ETH_P_ALL, SOCK_RAW, SOL_PACKET,
};
pub use libc::{AF_PACKET, IFF_PROMISC, PF_PACKET};

use std::ffi::CString;
use std::io::{self, Error, ErrorKind};
use std::mem;

const IFNAMESIZE: usize = 16;
const IFREQUNIONSIZE: usize = 24;

const SIOCGIFFLAGS: c_ulong = 35091; //0x00008913;
const SIOCSIFFLAGS: c_ulong = 35092; //0x00008914;

pub const PACKET_FANOUT: c_int = 18;

#[derive(Clone, Debug)]
#[repr(C)]
struct IfReq {
    ifr_name: [c_char; IFNAMESIZE],
    union: IfReqUnion,
}

#[derive(Clone, Debug)]
#[repr(C)]
struct IfReqUnion {
    data: [u8; IFREQUNIONSIZE],
}

impl Default for IfReqUnion {
    fn default() -> IfReqUnion {
        IfReqUnion { data: [0; IFREQUNIONSIZE] }
    }
}

impl IfReqUnion {
    fn as_short(&self) -> c_short {
        c_short::from_be((self.data[0] as c_short) << 8 | (self.data[1] as c_short))
    }

    fn from_short(i: c_short) -> IfReqUnion {
        let mut union = IfReqUnion::default();
        let bytes: [u8; 2] = unsafe { mem::transmute(i) };
        union.data[0] = bytes[0];
        union.data[1] = bytes[1];
        union
    }
}

impl IfReq {
    fn with_if_name(if_name: &str) -> io::Result<IfReq> {
        let mut if_req = IfReq::default();

        if if_name.len() >= if_req.ifr_name.len() {
            return Err(Error::new(ErrorKind::Other, "Interface name too long"));
        }

        // basically a memcpy
        for (a, c) in if_req.ifr_name.iter_mut().zip(if_name.bytes()) {
            *a = c as i8;
        }

        Ok(if_req)
    }

    fn ifr_flags(&self) -> c_short {
        self.union.as_short()
    }
}

impl Default for IfReq {
    fn default() -> IfReq {
        IfReq { ifr_name: [0; IFNAMESIZE], union: IfReqUnion::default() }
    }
}

#[derive(Clone, Debug)]
pub struct Socket {
    ///File descriptor
    pub fd: c_int,
    ///Interface name
    pub if_name: String,
    pub if_index: c_uint,
    pub sock_type: c_int,
}

impl Socket {
    pub fn from_if_name(if_name: &str, socket_type: c_int) -> io::Result<Socket> {
        //this typecasting sucks :(
        let fd = unsafe { socket(socket_type, SOCK_RAW, (ETH_P_ALL as u16).to_be() as i32) };
        if fd < 0 {
            return Err(Error::last_os_error());
        }

        Ok(Socket {
            if_name: String::from(if_name),
            if_index: get_if_index(if_name)?,
            sock_type: socket_type,
            fd,
        })
    }

    fn ioctl(&self, ident: c_ulong, if_req: IfReq) -> io::Result<IfReq> {
        let mut req: Box<IfReq> = Box::new(if_req);
        let retval = match () {
            #[cfg(target_arch = "mips")]
            _ => unsafe { ioctl(self.fd, ident as c_int, &mut *req) },
            #[cfg(target_arch = "x86_64")]
            _ => unsafe { ioctl(self.fd, ident, &mut *req) },
        };
        match retval {
            -1 => Err(Error::last_os_error()),
            _ => Ok(*req),
        }
    }

    fn get_flags(&self) -> io::Result<IfReq> {
        self.ioctl(SIOCGIFFLAGS, IfReq::with_if_name(&self.if_name)?)
    }

    pub fn set_flag(&mut self, flag: c_ulong) -> io::Result<()> {
        let flags = &self.get_flags()?.ifr_flags();
        let new_flags = flags | flag as c_short;
        let mut if_req = IfReq::with_if_name(&self.if_name)?;
        if_req.union.data = IfReqUnion::from_short(new_flags).data;
        self.ioctl(SIOCSIFFLAGS, if_req)?;
        Ok(())
    }

    pub fn setsockopt<T>(&mut self, opt: c_int, opt_val: T) -> io::Result<()> {
        match unsafe {
            setsockopt(
                self.fd,
                SOL_PACKET,
                opt,
                &opt_val as *const _ as *const c_void,
                mem::size_of_val(&opt_val) as socklen_t,
            )
        } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn getsockopt(&mut self, opt: c_int, opt_val: &*mut c_void) -> io::Result<()> {
        get_sock_opt(self.fd, opt, opt_val)
    }
}

pub fn get_sock_opt(fd: i32, opt: c_int, opt_val: &*mut c_void) -> io::Result<()> {
    let mut optlen = mem::size_of_val(&opt_val) as socklen_t;
    match unsafe { getsockopt(fd, SOL_PACKET, opt, *opt_val, &mut optlen) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

pub fn get_if_index(name: &str) -> io::Result<c_uint> {
    let name = CString::new(name)?;
    let index = unsafe { if_nametoindex(name.as_ptr()) };
    Ok(index)
}
