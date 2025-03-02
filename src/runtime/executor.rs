use std::{
    os::fd::RawFd,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
    task::{ContextBuilder, Waker},
    thread,
};

use fxhash::FxHashMap;

use super::task::{ID, Task, get_id};
use crate::{
    runtime::task::ExtData,
    utils::{
        NOW,
        epoll::{self, EpollEvent},
        pipe::Pipe,
    },
};

/// 总任务添加数量
pub static TASK_COUNT: AtomicU64 = AtomicU64::new(0);

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
            let count = TASK_COUNT.load(std::sync::atomic::Ordering::Relaxed);
            for work in &self.thread_pool.wokers {
                if !work.idle.load(std::sync::atomic::Ordering::Relaxed) {
                    continue 'a;
                }
            }
            if count == TASK_COUNT.load(std::sync::atomic::Ordering::Relaxed) {
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
    pipe_write: Arc<Pipe<Task>>,
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
                let mut map = FxHashMap::<ID, Task>::default();
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
                    if map.is_empty() {
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
