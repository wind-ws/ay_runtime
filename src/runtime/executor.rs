use std::{
    alloc::{Layout, dealloc},
    collections::HashMap,
    io::Write,
    os::fd::RawFd,
    pin::{Pin, pin},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize},
    },
    task::{Context, ContextBuilder, Waker},
    thread,
};

use lazy_static::lazy_static;

use super::{
    reactor::{Reactor, Register},
    task::{ID, Task, TaskWaker, get_id},
};
use crate::utils::{
    NOW,
    epoll::{self, EpollEvent},
    pipe::{self, Pipe},
};

static mut TASK_PIPE: Option<Arc<Pipe<Task>>> = None;
/// 总任务添加数量
static TASK_COUNT: AtomicU64 = AtomicU64::new(0);

pub struct Executor {
    thread_pool: ThreadPool,
    reactor: Reactor,
}

impl Executor {
    // 启动的线程数量
    pub fn new(n: usize) -> Self {
        let reactor = Reactor::new();
        let mut pool = ThreadPool {
            wokers: Vec::with_capacity(n),
            pipe: Arc::new(Pipe::new()),
        };
        for i in 0..n {
            let woker = Woker::new(
                i as ID,
                pool.pipe.clone(),
                reactor.get_pipe_write(),
            );
            pool.wokers.push(woker);
        }
        unsafe {
            TASK_PIPE = Some(pool.pipe.clone());
        }
        Self {
            thread_pool: pool,
            reactor,
        }
    }
    pub fn add_task(&self, task: &Task) {
        unsafe {
            TASK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        };
        self.thread_pool.pipe.write(task);
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
            TASK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        };
        #[allow(static_mut_refs)]
        unsafe {
            TASK_PIPE.as_ref().unwrap().write(&task)
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
    /// 只负责接受外部的Future到woker中poll
    pub pipe: Arc<Pipe<Task>>,
}
pub struct Woker {
    id: ID,
    thread: thread::JoinHandle<()>,
    /// true: 空闲
    pub idle: Arc<AtomicBool>,
}
impl Woker {
    pub fn new(
        id: ID,
        pipe: Arc<Pipe<Task>>,
        reg_pipe: Arc<Pipe<Register>>,
    ) -> Self {
        let idle = Arc::new(AtomicBool::new(false));
        let idle_ = idle.clone();
        let thread = thread::Builder::new()
            // .stack_size(1024 * 1024 * 20)
            .name(format!("work_thread[{}]", id));
        let thread = thread
            .spawn(move || {
                let task_pipe_reader = pipe;
                let mut map = HashMap::<ID, (Task, Waker)>::new();
                let id_pipe: Arc<Pipe<ID>> = Arc::new(pipe::Pipe::<ID>::new());
                let reg_pipe = reg_pipe;
                let idle = idle_;
                // 空闲多少ms,标记为空闲
                const IDEL_MS: u128 = 100;
                let mut time_anchor = 0u128;
                loop {
                    let tasks = task_pipe_reader.read_limited(100);
                    // println!("{:?}",tasks.len());
                    tasks.into_iter().for_each(|mut task| {
                        let waker =
                            Waker::from(Arc::new(task.waker(id_pipe.clone())));
                        let mut ed = ExtData {
                            id: task.id,
                            pipe_write: reg_pipe.clone(),
                        };
                        let mut cx = ContextBuilder::from_waker(&waker)
                            .ext(&mut ed)
                            .build();
                        let a = unsafe { task.future.as_mut() };
                        let f = Pin::static_mut(a);
                        // println!(" {}",task.id);
                        match f.poll(&mut cx) {
                            std::task::Poll::Ready(v) => {
                                println!("r {}",task.id);
                                
                                // 释放 future
                                unsafe {
                                    drop(Box::from_raw(task.future.as_ptr()))
                                }
                            }
                            std::task::Poll::Pending => {
                                println!("p {}",task.id);
                                map.insert(task.id, (task, waker));
                            }
                        }
                    });
                    let ids = id_pipe.read_all();
                    for id in ids.into_iter() {
                        let (task, waker) = map.get_mut(&id).unwrap();
                        let waker: Waker = waker.clone();
                        let mut ed = ExtData {
                            id: task.id,
                            pipe_write: reg_pipe.clone(),
                        };
                        let mut cx = ContextBuilder::from_waker(&waker)
                            .ext(&mut ed)
                            .build();

                        let a = unsafe { task.future.as_mut() };
                        let f = Pin::static_mut(a);

                        match f.poll(&mut cx) {
                            std::task::Poll::Ready(_v) => {
                                // println!("r {}|",id);
                                // 释放future
                                unsafe {
                                    drop(Box::from_raw(task.future.as_ptr()))
                                }
                                map.remove(&id);
                            }
                            std::task::Poll::Pending => {
                                // println!("p {}|",id);
                            }
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
        Self { id, thread, idle }
    }
}

pub struct ExtData {
    pub id: ID,
    pub pipe_write: Arc<Pipe<Register>>,
}
