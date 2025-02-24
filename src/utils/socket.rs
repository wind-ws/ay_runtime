use std::{net::SocketAddr, ops::Deref, os::fd::RawFd};

// struct Socket {
//     socket_fd: RawFd,
// }
// impl Socket {
//     pub fn new(domain: Domain, ty: Type, protocol: Option<Protocol>) {
//         // libc::socket(*domain, *ty, 0);
//     }
// }

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Domain(pub libc::c_int);
impl Domain {
    pub const IPV4: Domain = Domain(libc::AF_INET);

    pub const IPV6: Domain = Domain(libc::AF_INET6);

    pub const UNIX: Domain = Domain(libc::AF_UNIX);
    
    pub const fn for_address(address: SocketAddr) -> Domain {
        match address {
            SocketAddr::V4(_) => Domain::IPV4,
            SocketAddr::V6(_) => Domain::IPV6,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Type(pub libc::c_int);

impl Type {

    pub const STREAM: Type = Type(libc::SOCK_STREAM);

    pub const DGRAM: Type = Type(libc::SOCK_DGRAM);


}


#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Protocol(pub libc::c_int);

impl Protocol {

    pub const TCP: Protocol = Protocol(libc::IPPROTO_TCP);


}
