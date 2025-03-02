// use std::{ffi::CString, os::fd::RawFd};

// use libc::*;

// type MqAttr = libc::mq_attr;

// /// `max` : 队列最大消息数
// pub fn mq_open_create<T>(name: &str, max: i64) -> RawFd {
//     let size = std::mem::size_of::<T>();
//     let name = CString::new(name).unwrap();
//     unsafe {
//         let mut attr = std::mem::zeroed::<MqAttr>();
//         attr.mq_flags = (O_CREAT | O_RDWR | O_NONBLOCK) as i64;
//         attr.mq_maxmsg = max;
//         attr.mq_msgsize = size as i64;
//         attr.mq_curmsgs = 0;
//         let res = libc::mq_open(
//             name.as_ptr() as *const c_char,
//             O_CREAT | O_RDWR | O_NONBLOCK,
//             S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH | S_IWOTH,
//             // &mut attr as *mut mq_attr,
//             std::ptr::null_mut() as *mut mq_attr,
//         );
//         if res == -1 {
//             let err = std::io::Error::last_os_error();
//             panic!("{:#?}", err)
//         } else {
//             // mq_setattr(
//             //     res,
//             //     &mut attr as *mut mq_attr,
//             //     std::ptr::null_mut() as *mut mq_attr,
//             // );
//         }
//         res
//     }
// }

// /// # Safety
// /// 拷贝地址的连续数据,注意 Rust和C交互的 堆自动释放问题
// /// 如果T的字段中,存在 引用或指针 ,你需要保证他们在 发送后,直到被接受后 仍然存活
// pub fn send<T>(fd: RawFd, t: &T) -> i32 {
//     let size = std::mem::size_of::<T>();
//     unsafe {
//         let res = mq_send(fd, t as *const T as *const c_char, size, 0);
//         if res == -1 {
//             let err = std::io::Error::last_os_error();
//             if !(err.raw_os_error() == Some(libc::EINPROGRESS)
//                 || err.kind() == std::io::ErrorKind::WouldBlock)
//             {
//                 panic!("{:#?}", err)
//             }
//         }
//         res
//     }
// }

// pub fn receive<T>(fd: RawFd, max: usize) -> Vec<T> {
//     let size = std::mem::size_of::<T>();
//     let mut vec = Vec::<T>::with_capacity(max);
//     unsafe {
//         let res = mq_receive(
//             fd,
//             vec.as_mut_ptr() as *mut c_char,
//             size * max,
//             &mut 0u32 as *mut u32,
//         );
//         if res == 0 {
//             vec
//         } else if res as usize % size == 0 {
//             vec.set_len(res as usize % size);
//             vec
//         } else {
//             let err = std::io::Error::last_os_error();
//             panic!("{:#?}", err)
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test() {
//         let mq = mq_open_create::<u32>("/myque", 1);

//         let mut a = 0;
//         let mut b = 1u32;
//         unsafe {
//             loop {
//                 libc::mq_send(
//                     mq,
//                     &(b as i32) as *const i32 as *const c_char,
//                     4,
//                     1,
//                 );

//                 libc::mq_receive(
//                     mq,
//                     &mut a as *mut i32 as *mut c_char,
//                     4,
//                     &mut b as *mut u32 as *mut c_uint,
//                 );
//                 if a != 0 {
//                     println!("{:?}", a);
//                     break;
//                 }
//             }

//         }
//     }
// }
