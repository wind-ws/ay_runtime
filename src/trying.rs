// Executor 负责分配 Future 给 A线程 执行

// Executor 如何分配任务:
// 只管将Task 推入 pipe,
// 由A线程去争抢任务,

// id 全局唯一(不回收)

// Reactor 负责捕捉epoll事件 和 调用对应waker
//

// waker 负责 告诉A线程再次执行 对应Future

// A线程 负责执行和管理Future

// Future::poll 负责 将waker进注册 reactor 由epoll事件触发执行waker
//
// 调用Future::poll 时的 Context 附带 context_ext(扩展数据)(包含: id,reactor_writer )
// 若 poll Ready,则 消耗 对应 id 的 Future,
// 若 poll Pending,则 通过 epoll 注册 需要的 interest_fd 且 附带id,

// 无论如何,都需要 线程信息传输,方案有一下几种:
// 2. 使用 pipe + epoll 通信

use std::{
    collections::HashMap,
    os::fd::RawFd,
    pin::Pin,
    ptr::NonNull,
    sync::{Arc, atomic::AtomicBool},
    task::{Context, ContextBuilder, Waker},
    thread,
};

use lazy_static::lazy_static;

use crate::{
    runtime::{executor::TASK_COUNT, task::get_id},
    utils::{
        NOW,
        epoll::{self, EpollEvent},
        pipe::Pipe,
    },
};

pub type ID = u64;

pub static mut EXECUTOR: Option<Arc<ThreadPool>> = None;

pub struct Executor {
    thread_pool: Arc<ThreadPool>,
}

impl Executor {
    // 启动的线程数量
    pub fn new(n: usize) -> Self {
        let mut pool = ThreadPool {
            wokers: Vec::with_capacity(n),
        };
        for i in 0..n {
            let woker = Woker::new(i as ID);
            pool.wokers.push(woker);
        }
        let pool = Arc::new(pool);
        unsafe {
            EXECUTOR = Some(pool.clone());
        };

        Self { thread_pool: pool }
    }
    pub fn add_task(&self, task: &Task) {
        self.thread_pool.add_task(task);
    }

    /// #plan : 获取返回值
    pub fn spawn<F>(future: F)
    where
        F: Future<Output = ()> + Send + 'static,
        F::Output: Send + 'static,
    {
        let id = get_id();
        let task = Task::new(id, Box::new(future));
        unsafe {
            #[allow(static_mut_refs)]
            EXECUTOR.as_ref().unwrap().add_task(&task);
        };
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        todo!()
    }
    pub fn block(&self) {
        'a: loop {
            let count = unsafe {
                TASK_COUNT.load(std::sync::atomic::Ordering::Relaxed)
            };
            for work in &self.thread_pool.wokers {
                if !work.idle.load(std::sync::atomic::Ordering::Relaxed) {
                    continue 'a;
                }
            }
            if count
                == unsafe {
                    TASK_COUNT.load(std::sync::atomic::Ordering::Relaxed)
                }
            {
                break;
            }
        }
    }
}

pub struct ThreadPool {
    pub wokers: Vec<Woker>,
}
impl ThreadPool {
    pub fn add_task(&self, task: &Task) {
        let len = self.wokers.len();
        let n = rand::random_range(0usize..len);
        self.wokers[n].pipe_write.write(task);
    }
}

pub struct Woker {
    id: ID,
    epoll_fd: RawFd,
    thread: thread::JoinHandle<()>,
    pub pipe_write: Arc<Pipe<Task>>,
    /// true: 空闲
    pub idle: Arc<AtomicBool>,
}

impl Woker {
    pub fn new(id: ID) -> Self {
        let epoll_fd = epoll::create().unwrap();
        let idle = Arc::new(AtomicBool::new(false));
        let idle_ = idle.clone();
        let pipe_write = Arc::new(Pipe::<Task>::new());
        let pipe_reader = pipe_write.clone();
        let thread = thread::Builder::new()
            // .stack_size(1024 * 1024 * 20)
            .name(format!("work_thread[{}]", id));
        let thread = thread
            .spawn(move || {
                let task_pipe_reader = pipe_reader;
                let epoll_fd = epoll_fd;
                let mut epoll_events: Vec<EpollEvent> =
                    Vec::with_capacity(1024);
                let mut map = HashMap::<ID, Task>::new();
                let idle = idle_;
                // 空闲多少ms,标记为空闲
                const IDEL_MS: u128 = 10;
                let mut time_anchor = 0u128;
                loop {
                    let tasks = task_pipe_reader.read_all();
                    for mut task in tasks {
                        let waker = Waker::noop();
                        let mut ed = ExtData {
                            epoll_fd,
                            id: task.id,
                        };
                        let mut cx = ContextBuilder::from_waker(waker)
                            .ext(&mut ed)
                            .build();
                        let a = unsafe { task.future.as_mut() };
                        let f = Pin::static_mut(a);
                        match f.poll(&mut cx) {
                            std::task::Poll::Ready(v) => {
                                // 释放 future
                                task.drop();
                            }
                            std::task::Poll::Pending => {
                                // println!("frist:{}", task.id);
                                map.insert(task.id, task);
                            }
                        }
                    }
                    let n = epoll::wait(epoll_fd, &mut epoll_events, 1024, 0)
                        .unwrap();
                    let n = n as usize;
                    unsafe { epoll_events.set_len(n) };
                    // 注册 epoll event 和 注销 epoll event 都在 Future poll中执行
                    for event in &epoll_events[0..n] {
                        let id = event.u64;
                        let task = map.get_mut(&id).unwrap();
                        let waker = Waker::noop();
                        let mut ed = ExtData {
                            epoll_fd,
                            id: task.id,
                        };
                        let mut cx = ContextBuilder::from_waker(waker)
                            .ext(&mut ed)
                            .build();
                        let a = unsafe { task.future.as_mut() };
                        let f = Pin::static_mut(a);
                        // println!("happen:{}", task.id);
                        match f.poll(&mut cx) {
                            std::task::Poll::Ready(v) => {
                                // println!("remove:{}", task.id);
                                // 释放 future
                                map.remove(&id).unwrap().drop();
                            }
                            std::task::Poll::Pending => {}
                        }
                    }
                    // true:空闲
                    if map.len() == 0 {
                        let now = NOW.elapsed().as_millis();
                        if time_anchor == 0 {
                            time_anchor = now;
                        }
                        if now - time_anchor >= IDEL_MS {
                            idle.store(
                                true,
                                std::sync::atomic::Ordering::Relaxed,
                            );
                        }
                    } else {
                        time_anchor = 0;
                    }
                }
            })
            .unwrap();
        Self {
            epoll_fd,
            id,
            thread,
            idle,
            pipe_write,
        }
    }
}

pub struct Task<O = ()> {
    pub id: ID,
    pub future: NonNull<(dyn Future<Output = O> + Send)>,
}
impl Task {
    pub fn new(id: ID, future: Box<dyn Future<Output = ()> + Send>) -> Self {
        let ptr = Box::into_raw(future);
        Self {
            id,
            future: NonNull::new(ptr).unwrap(),
        }
    }
    pub fn drop(self) {
        drop(unsafe { Box::from_raw(self.future.as_ptr()) })
    }
}

pub struct ExtData {
    pub epoll_fd: RawFd,
    pub id: ID,
}

mod tcp {
    use std::{
        ffi::c_void,
        io::{self, Error},
        net::ToSocketAddrs,
        task::Poll,
    };

    use super::*;
    use crate::utils::socket::{Domain, Protocol, Type};

    pub struct SleepFd {
        fd: i32,
        ms: u64,
        b: bool,
    }
    impl SleepFd {
        pub fn new(ms: u64) -> Self {
            Self {
                fd: unsafe {
                    libc::timerfd_create(
                        libc::CLOCK_MONOTONIC,
                        libc::TFD_NONBLOCK,
                    )
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
                        tv_nsec: 1000_000 * self.ms as i64,
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
            // let socket_addr = addr.to_socket_addrs()?.next().unwrap();
            let domain = Domain::IPV4; // Domain::for_address(socket_addr);

            let sfd = unsafe { libc::socket(domain.0, ty.0, protocol.0) };
            if sfd < 0 {
                return Err(Error::last_os_error());
            }
            let flags = unsafe { libc::fcntl(sfd, libc::F_GETFL, 0) };
            let new_flags = flags | libc::O_NONBLOCK;
            unsafe { libc::fcntl(sfd, libc::F_SETFL, new_flags) };

            // match socket_addr {
            //     SocketAddr::V4(socket_addr_v4) => {
            let sockaddr = libc::sockaddr_in {
                sin_family: libc::AF_INET as u16,
                sin_port: 3000u16.to_be(), //socket_addr_v4.port().to_be(),
                sin_addr: libc::in_addr {
                    s_addr: u32::from_be_bytes(
                        [127, 0, 0, 1], //socket_addr_v4.ip().octets(),
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
            //     }
            //     SocketAddr::V6(socket_addr_v6) => todo!(),
            // };
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

        fn poll(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Self::Output> {
            unsafe {
                let ext = cx.ext().downcast_mut::<ExtData>().unwrap();

                let res = libc::connect(
                    self.socket,
                    &self.addr as *const libc::sockaddr_in
                        as *const libc::sockaddr,
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

        fn poll(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Self::Output> {
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
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::{Executor, Task, Woker, tcp::SleepFd};
    use crate::{trying::tcp::TcpStream, utils::NOW};

    #[test]
    fn test() {
        let worker = Woker::new(1);
        for i in 0..10000 {
            let future = async move {
                println!("start:{:?}", NOW.elapsed());
                SleepFd::new(100).await;
                println!("done[{}]:{:?}", i, NOW.elapsed());
            };
            let task = Task::new(i, Box::new(future));
            worker.pipe_write.write(&task);
        }

        thread::sleep(Duration::from_millis(1000));
    }

    #[test]
    fn test2() {
        let executor = Executor::new(10);
        for i in 0..100000 {
            let future = async move {
                println!("start:{:?}", NOW.elapsed());
                SleepFd::new(100).await;
                println!("done[{}]:{:?}", i, NOW.elapsed());
            };
            // let task = Task::new(i, Box::new(future));
            Executor::spawn(future);
        }
        executor.block();
    }

    #[test]
    fn test3() {
        let executor = Executor::new(10);
        for i in 0..1000 {
            let future = async move {
                // println!("start:{:?}", NOW.elapsed());
                let thread = thread::current();
                let stream =
                    TcpStream::connect("127.0.0.1:3000").await.unwrap();
                let mut buf = [0u8; 10];
                buf[0] = i as u8;
                stream.write(&buf).await.unwrap();
                stream.read(&mut buf).await.unwrap();
                println!(
                    "{:?} done{}[{:?}]:{:?}",
                    thread.name().unwrap(),
                    i,
                    NOW.elapsed(),
                    buf
                );
            };
            // let task = Task::new(i, Box::new(future));
            Executor::spawn(future);
        }
        // executor.block();
        thread::sleep(Duration::from_millis(1000));
    }
}
