use std::{
    ffi::c_void,
    marker::PhantomData,
    mem::MaybeUninit,
    os::fd::{self, RawFd},
};

/// 创建非堵塞pipe
///
/// @return (read_fd,write_fd)
pub(crate) fn pipe2() -> (RawFd, RawFd) {
    let mut fds: [RawFd; 2] = [0; 2];
    unsafe {
        libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK);
    }
    // let current_size = unsafe { libc::fcntl(fds[1], libc::F_GETPIPE_SZ) };
    // let new_size = 1024 * 1024; // 128KB 原 64KB
    // if unsafe { libc::fcntl(fds[1], libc::F_SETPIPE_SZ, new_size) } == -1 {
    //     panic!("")
    // }
    // if unsafe { libc::fcntl(fds[0], libc::F_SETPIPE_SZ, new_size) } == -1 {
    //     panic!("")
    // }

    (fds[0], fds[1])
}
//dup / dup2: 复制 pipe 的读端或写端。
pub(crate) struct Pipe<T> {
    read_fd: RawFd,
    write_fd: RawFd,
    _marker: PhantomData<T>,
}
unsafe impl<T> Send for Pipe<T> {}
unsafe impl<T> Sync for Pipe<T> {}

impl<T> Pipe<T> {
    pub fn new() -> Self {
        let (read_fd, write_fd) = pipe2();
        Self {
            read_fd,
            write_fd,
            _marker: PhantomData::<T>,
        }
    }
    pub fn write(&self, v: &T) {
        let len = std::mem::size_of::<T>();
        unsafe {
            let n =
                libc::write(self.write_fd, v as *const T as *const c_void, len);
            if n != len as isize {
                panic!("Pipe::write write amount err {}", n)
            }
        };
    }
    pub fn read(&self) -> Option<T> {
        let len = std::mem::size_of::<T>();
        let mut v: MaybeUninit<T> = MaybeUninit::uninit();
        unsafe {
            let n =
                libc::read(self.read_fd, v.as_mut_ptr() as *mut c_void, len);
            if n == 0 || n == -1 {
                None
            } else if n == len as isize {
                Some(v.assume_init())
            } else {
                panic!("Pipe::read read amount err {}", n);
            }
        }
    }
    pub fn read_all(&self) -> Vec<T> {
        // #plan : 不调用read,而是获取批量T
        // let len = std::mem::size_of::<T>();
        let mut vec = Vec::<T>::new();
        while let Some(v) = self.read() {
            vec.push(v);
        }
        vec
    }
    ///
    ///
    /// `n` : 最大读取数量
    ///
    /// @return iter
    pub fn read_limited(&self, n: usize) -> Vec<T> {
        let len = std::mem::size_of::<T>();
        let mut vec = Vec::<T>::with_capacity(n);
        unsafe {
            let mut n = libc::read(
                self.read_fd,
                vec.as_mut_ptr() as *mut c_void,
                len * n,
            );
            if n < 0 {
                n = 0;
            }
            if n as usize % len != 0 {
                panic!("")
            }
            vec.set_len(n as usize / len);
            vec
        }
    }
}

impl<T> Drop for Pipe<T> {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.read_fd);
            libc::close(self.write_fd);
        };
    }
}

#[cfg(test)]
mod tests {
    use super::Pipe;
    #[derive(Debug)]
    struct A {
        u: u32,
        b: bool,
    }
    #[test]
    fn test() {
        let pipe = Pipe::<A>::new();
        pipe.write(&A { u: 1, b: false });
        pipe.write(&A { u: 2, b: false });
        pipe.write(&A { u: 3, b: false });
        pipe.write(&A { u: 4, b: false });
        pipe.write(&A { u: 5, b: false });
        pipe.write(&A { u: 6, b: false });
        println!("{:?}", pipe.read_limited(5));

        println!("{:?}", pipe.read_all());
    }
    #[test]
    fn test_mutil() {}
}
