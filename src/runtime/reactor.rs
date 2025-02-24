use std::{os::fd::RawFd, sync::Arc, task::Waker, thread};

use fxhash::FxHashMap;

use super::task::{ID, TaskWaker};
use crate::utils::{
    epoll::{self, EpollEvent},
    pipe::{self, Pipe},
};

pub struct Register {
    pub id: ID,
    pub interest_fd: RawFd,
    pub events: u32,
    pub waker: Waker,
}
pub struct Reactor {
    efd: RawFd,
    pipe: Arc<Pipe<Register>>,
    thread: thread::JoinHandle<()>,
}

impl Reactor {
    pub fn new() -> Self {
        let efd = epoll::create().unwrap();
        let pipe = Arc::new(pipe::Pipe::<Register>::new());
        let pipe_reader = pipe.clone();
        let thread = thread::spawn(move || {
            let efd = efd;
            let pipe_reader = pipe_reader;
            let mut epoll_events: Vec<EpollEvent> = Vec::with_capacity(1024);
            let mut map = FxHashMap::<ID, Register>::default();
            loop {
                let n = epoll::wait(efd, &mut epoll_events, 1024, 0).unwrap();
                let n = n as usize;
                // 接受事件,并注册到epoll
                pipe_reader.read_all().into_iter().for_each(|reg| {
                    epoll::register(efd, reg.events, reg.interest_fd, reg.id)
                        .unwrap();
                    println!("isert:{}", reg.id);
                    map.insert(reg.id, reg);
                });
                unsafe { epoll_events.set_len(n) };
                // 被触发的事件id
                for event_id in &epoll_events[0..n] {
                    let event_id = event_id.u64;
                    println!("happen:{}", event_id);
                    if let Some(reg) = map.remove(&event_id) {
                        println!("remove:{}", event_id);
                        epoll::unregister(
                            efd,
                            reg.events,
                            reg.interest_fd,
                            event_id,
                        )
                        .unwrap();
                        reg.waker.wake();
                    }
                }
            }
        });
        Self { efd, pipe, thread }
    }

    pub fn get_pipe_write(&self) -> Arc<Pipe<Register>> {
        self.pipe.clone()
    }
}
