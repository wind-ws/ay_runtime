use std::{
    ffi::c_void,
    io::{self, Error},
    net::{SocketAddr, ToSocketAddrs},
    os::fd::RawFd,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    runtime::task::ExtData,
    utils::{
        epoll,
        socket::{Domain, Protocol, Type},
    },
};

pub struct SleepFd {
    fd: i32,
    ms: u64,
    b: bool,
}
impl SleepFd {
    pub fn new(ms: u64) -> Self {
        Self {
            fd: unsafe {
                libc::timerfd_create(libc::CLOCK_MONOTONIC, libc::TFD_NONBLOCK)
            },
            ms,
            b: false,
        }
    }
}
impl Future for SleepFd {
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let ext = cx.ext().downcast_mut::<ExtData>().unwrap();
        if self.b {
            epoll::unregister(
                ext.epoll_fd,
                (libc::EPOLLIN) as u32,
                self.fd,
                ext.id,
            )
            .unwrap();
            Poll::Ready(())
        } else {
            self.b = true;
            let timer_spec = libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                }, // 无间隔
                it_value: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 1_000_000 * self.ms as i64,
                },
            };
            unsafe {
                libc::timerfd_settime(
                    self.fd,
                    0,
                    &timer_spec,
                    std::ptr::null_mut(),
                )
            };
            epoll::register(
                ext.epoll_fd,
                (libc::EPOLLIN) as u32,
                self.fd,
                ext.id,
            )
            .unwrap();

            Poll::Pending
        }
    }
}

pub struct TcpStream {
    socket_fd: RawFd,
    // addr: libc::sockaddr,
}
impl TcpStream {
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let ty = Type::STREAM;
        let protocol = Protocol::TCP;
        let socket_addr = addr.to_socket_addrs()?.next().unwrap();
        let domain = Domain::IPV4; // Domain::for_address(socket_addr);

        let sfd = unsafe { libc::socket(domain.0, ty.0, protocol.0) };
        if sfd < 0 {
            return Err(Error::last_os_error());
        }
        let flags = unsafe { libc::fcntl(sfd, libc::F_GETFL, 0) };
        let new_flags = flags | libc::O_NONBLOCK;
        unsafe { libc::fcntl(sfd, libc::F_SETFL, new_flags) };

        match socket_addr {
            SocketAddr::V4(socket_addr_v4) => {
                let sockaddr = libc::sockaddr_in {
                    sin_family: libc::AF_INET as u16,
                    sin_port: socket_addr_v4.port().to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_be_bytes(
                            socket_addr_v4.ip().octets(),
                        )
                        .to_be(),
                    },
                    sin_zero: [0; 8],
                };
                ConnectTcpStreamFuture {
                    socket: sfd as RawFd,
                    addr: sockaddr,
                    len: std::mem::size_of::<libc::sockaddr_in>() as u32,
                }
                .await?;
            }
            SocketAddr::V6(_socket_addr_v6) => todo!(),
        };
        Ok(Self {
            socket_fd: sfd,
            // addr: sockaddr,
        })
    }

    pub async fn read(&self, buf: &mut [u8]) -> io::Result<()> {
        ReadTcpStreamFuture {
            socket: self.socket_fd,
            buf,
        }
        .await
    }

    pub async fn write(&self, buf: &[u8]) -> io::Result<()> {
        WriteTcpStreamFuture {
            socket: self.socket_fd,
            buf,
        }
        .await
    }
}
pub struct ConnectTcpStreamFuture {
    socket: RawFd,
    addr: libc::sockaddr_in,
    len: u32,
}
impl Future for ConnectTcpStreamFuture {
    type Output = Result<(), io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let ext = cx.ext().downcast_mut::<ExtData>().unwrap();

            let res = libc::connect(
                self.socket,
                &self.addr as *const libc::sockaddr_in as *const libc::sockaddr,
                self.len,
            );
            if res == 0 {
                epoll::unregister(
                    ext.epoll_fd,
                    (libc::EPOLLOUT) as u32,
                    self.socket,
                    ext.id,
                )
                .unwrap();
                Poll::Ready(Ok(()))
            } else {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINPROGRESS)
                    || err.kind() == io::ErrorKind::WouldBlock
                {
                    epoll::register(
                        ext.epoll_fd,
                        (libc::EPOLLOUT) as u32,
                        self.socket,
                        ext.id,
                    )
                    .unwrap();
                    Poll::Pending
                } else {
                    Poll::Ready(Err(err))
                }
            }
        }
    }
}

pub struct ReadTcpStreamFuture<'a> {
    socket: i32,
    buf: &'a mut [u8],
}
impl Future for ReadTcpStreamFuture<'_> {
    type Output = io::Result<()>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        unsafe {
            let ext = cx.ext().downcast_mut::<ExtData>().unwrap();

            let res = libc::read(
                self.socket,
                self.buf.as_mut_ptr() as *mut c_void,
                self.buf.len(),
            );
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINPROGRESS)
                    || err.kind() == io::ErrorKind::WouldBlock
                {
                    epoll::register(
                        ext.epoll_fd,
                        (libc::EPOLLIN) as u32,
                        self.socket,
                        ext.id,
                    )
                    .unwrap();
                    Poll::Pending
                } else {
                    epoll::unregister(
                        ext.epoll_fd,
                        (libc::EPOLLIN) as u32,
                        self.socket,
                        ext.id,
                    )
                    .unwrap();
                    Poll::Ready(Err(err))
                }
            } else {
                epoll::unregister(
                    ext.epoll_fd,
                    (libc::EPOLLIN) as u32,
                    self.socket,
                    ext.id,
                )
                .unwrap();
                Poll::Ready(Ok(()))
            }
        }
    }
}

pub struct WriteTcpStreamFuture<'a> {
    socket: i32,
    buf: &'a [u8],
}
impl Future for WriteTcpStreamFuture<'_> {
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let ext = cx.ext().downcast_mut::<ExtData>().unwrap();
            let res = libc::write(
                self.socket,
                self.buf.as_ptr() as *const c_void,
                self.buf.len(),
            );
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINPROGRESS)
                    || err.kind() == io::ErrorKind::WouldBlock
                {
                    epoll::register(
                        ext.epoll_fd,
                        (libc::EPOLLIN) as u32,
                        self.socket,
                        ext.id,
                    )
                    .unwrap();
                    Poll::Pending
                } else {
                    epoll::unregister(
                        ext.epoll_fd,
                        (libc::EPOLLIN) as u32,
                        self.socket,
                        ext.id,
                    )
                    .unwrap();
                    Poll::Ready(Err(err))
                }
            } else {
                epoll::unregister(
                    ext.epoll_fd,
                    (libc::EPOLLIN) as u32,
                    self.socket,
                    ext.id,
                )
                .unwrap();
                Poll::Ready(Ok(()))
            }
        }
    }
}
