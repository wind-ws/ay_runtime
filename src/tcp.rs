use std::{
    future::Pending,
    io::{self, Error},
    mem,
    net::{SocketAddr, ToSocketAddrs},
    os::fd::RawFd,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    thread,
    time::Duration,
};

use libc::sockaddr;

use crate::{
    runtime::{executor::ExtData, reactor::Register},
    utils::socket::{Domain, Protocol, Type},
};

pub struct TcpStream {
    socket_fd: RawFd,
    // addr: libc::sockaddr,
}
impl TcpStream {
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let ty = Type::STREAM;
        let protocol = Protocol::TCP;
        let socket_addr = addr.to_socket_addrs()?.next().unwrap();
        let domain = Domain::for_address(socket_addr);

        let sfd = unsafe { libc::socket(domain.0, ty.0, protocol.0) };
        if sfd < 0 {
            return Err(Error::last_os_error());
        }
        let flags = unsafe { libc::fcntl(sfd, libc::F_GETFL, 0) };
        let new_flags = flags | libc::O_NONBLOCK;
        unsafe { libc::fcntl(sfd, libc::F_SETFL, new_flags) };

        unsafe {
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
                    .await
                    .unwrap();
                }
                SocketAddr::V6(socket_addr_v6) => todo!(),
            }
        };

        Ok(Self {
            socket_fd: sfd,
            // addr: sockaddr,
        })
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
            let res = libc::connect(
                self.socket,
                &self.addr as *const libc::sockaddr_in as *const libc::sockaddr,
                self.len,
            );
            if res == 0 {
                Poll::Ready(Ok(()))
            } else {
                let err = io::Error::last_os_error();
                println!("{:?}", err);
                if err.raw_os_error() == Some(libc::EINPROGRESS)
                    || err.kind() == io::ErrorKind::WouldBlock
                {
                    let waker = cx.waker().clone();
                    let ext = cx.ext().downcast_mut::<ExtData>().unwrap();
                    ext.pipe_write.write(&Register {
                        id: ext.id,
                        interest_fd: self.socket,
                        events: (libc::EPOLLOUT) as u32,
                        waker,
                    });
                    Poll::Pending
                } else {
                    Poll::Ready(Err(err))
                }
            }
        }
    }
}

/// #plan : 改为timefd
pub struct Sleep {
    duration: Duration,
    completed: Arc<AtomicBool>,
    // 保证线程只运行一次
    b_thread: bool,
}
impl Sleep {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            completed: Arc::new(AtomicBool::new(false)),
            b_thread: true,
        }
    }
}
impl Future for Sleep {
    type Output = u128;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.completed.load(std::sync::atomic::Ordering::Relaxed) {
            Poll::Ready(self.duration.as_millis())
        } else {
            if self.b_thread {
                self.b_thread = false;
                let waker = cx.waker().clone();
                let ext = cx.ext().downcast_mut::<ExtData>().unwrap();
                let duration = self.duration;
                let completed = self.completed.clone();
                thread::spawn(move || {
                    thread::sleep(duration);
                    completed.store(true, Ordering::SeqCst);
                    waker.wake();
                });
            }
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        net::{TcpStream, ToSocketAddrs},
        os::fd::{AsFd, AsRawFd},
        pin::Pin,
        task::{Context, Poll},
        thread,
        time::Duration,
    };

    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    use crate::{
        runtime::{
            executor::{Executor, ExtData},
            reactor::Register,
            task::{get_id, Task},
        }, tcp::Sleep, utils::NOW
    };

    pub struct AsyncTcpStream {
        inner: TcpStream,
    }

    impl AsyncTcpStream {
        pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
            let sock_type = Type::STREAM;
            let protocol = Protocol::TCP;
            let socket_addr = addr.to_socket_addrs()?.next().unwrap();
            let domain = Domain::for_address(socket_addr);
            let socket = Socket::new(domain, sock_type, Some(protocol))
                .unwrap_or_else(|_| {
                    panic!("Failed to create socket at address {}", socket_addr)
                });

            socket.set_nonblocking(true).unwrap();
            ConnectTcpStreamFuture {
                socket: &socket,
                addr: SockAddr::from(socket_addr),
            }
            .await?;
            let stream = TcpStream::from(socket);
            // 也许没必要,它可能跟随socket
            stream.set_nonblocking(true)?;
            Ok(Self { inner: stream })
        }
    }

    pub struct ConnectTcpStreamFuture<'a> {
        socket: &'a Socket,
        addr: SockAddr,
    }
    impl Future for ConnectTcpStreamFuture<'_> {
        type Output = Result<(), io::Error>;

        fn poll(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Self::Output> {
            match self.socket.connect(&self.addr) {
                Ok(_) => Poll::Ready(Ok(())),
                Err(e)
                    if e.raw_os_error() == Some(libc::EINPROGRESS)
                        || e.kind() == io::ErrorKind::WouldBlock =>
                {
                    let waker = cx.waker().clone();
                    let ext = cx.ext().downcast_mut::<ExtData>().unwrap();
                    ext.pipe_write.write(&Register {
                        id: ext.id,
                        interest_fd: self.socket.as_raw_fd(),
                        events: (libc::EPOLLOUT) as u32,
                        waker,
                    });
                    Poll::Pending
                }
                Err(e) => Poll::Ready(Err(e)),
            }
        }
    }

    #[test]
    fn test() {
        let executor = Executor::new(10);
        let future = async {
            println!("start:{}", NOW.elapsed().as_millis());
            // Sleep::new(Duration::from_millis(100)).await;
            let stream =
                AsyncTcpStream::connect("127.0.0.1:3000").await.unwrap();
            println!("done:{}", NOW.elapsed().as_millis());
        };
        let task = Task {
            id: get_id(),
            future: Box::pin(future),
        };

        executor.add_task(&task);

        thread::sleep(Duration::from_millis(100001));
    }
}
