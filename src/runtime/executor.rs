use std::{
    collections::HashMap, io::Write, os::fd::RawFd, sync::Arc, task::{ContextBuilder, Waker}, thread
};

use super::{
    reactor::{Reactor, Register},
    task::{get_id, Task, TaskWaker, ID},
};
use crate::utils::{
    epoll::{self, EpollEvent},
    pipe::{self, Pipe},
};

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
        Self {
            thread_pool: pool,
            reactor,
        }
    }
    pub fn add_task(&self, task: &Task) {
        self.thread_pool.pipe.write(task);
    }
    // pub fn add_future(&self,future:){
    //     let id = get_id();
    // }
    pub fn spawn() {}
    pub fn block_on<F>(future: F)
    where
        F: Future,
    {
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
}
impl Woker {
    pub fn new(
        id: ID,
        pipe: Arc<Pipe<Task>>,
        reg_pipe: Arc<Pipe<Register>>,
    ) -> Self {
        let thread = thread::Builder::new()
            // .stack_size(1024 * 1024 * 20)
            .name(format!("work_thread[{}]", id));
        let thread = thread
            .spawn(move || {
                let task_pipe_reader = pipe;
                let mut map = HashMap::<ID, Task>::new();
                let id_pipe: Arc<Pipe<ID>> = Arc::new(pipe::Pipe::<ID>::new());
                let reg_pipe = reg_pipe;
                loop {
                    let tasks = task_pipe_reader.read_limited(5);
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
                        match task.future.as_mut().poll(&mut cx) {
                            std::task::Poll::Ready(v) => {}
                            std::task::Poll::Pending => {
                                map.insert(task.id, task);
                            }
                        }
                    });
                    let ids = id_pipe.read_all();
                    for id in ids.into_iter() {
                        let task = map.get_mut(&id).unwrap();
                        let b = task.waker(id_pipe.clone());
                        Arc::new(Arc::new(pipe::Pipe::<ID>::new()));
                        0;
                        let c = Arc::new(b);
                        let waker: Waker = Waker::from(c);
                        let mut ed = ExtData {
                            id: task.id,
                            pipe_write: reg_pipe.clone(),
                        };
                        let mut cx = ContextBuilder::from_waker(&waker)
                            .ext(&mut ed)
                            .build();
                        match task.future.as_mut().poll(&mut cx) {
                            std::task::Poll::Ready(v) => {
                                map.remove(&id);
                            }
                            std::task::Poll::Pending => {}
                        }
                    }
                    // ids.into_iter().for_each(|id| {
                        
                    // });
                }
            })
            .unwrap();
        Self { id, thread }
    }
}

pub struct ExtData {
    pub id: ID,
    pub pipe_write: Arc<Pipe<Register>>,
}
