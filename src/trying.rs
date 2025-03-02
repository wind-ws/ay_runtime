// #[cfg(test)]
// mod tests {
//     use std::{thread, time::Duration};

//     #[test]
//     fn test() {
//         let worker = Woker::new(1);
//         for i in 0..10000 {
//             let future = async move {
//                 println!("start:{:?}", NOW.elapsed());
//                 SleepFd::new(100).await;
//                 println!("done[{}]:{:?}", i, NOW.elapsed());
//             };
//             let task = Task::new(i, Box::new(future));
//             worker.pipe_write.write(&task);
//         }

//         thread::sleep(Duration::from_millis(1000));
//     }

//     #[test]
//     fn test2() {
//         let executor = Executor::new(10);
//         for i in 0..100000 {
//             let future = async move {
//                 println!("start:{:?}", NOW.elapsed());
//                 SleepFd::new(100).await;
//                 println!("done[{}]:{:?}", i, NOW.elapsed());
//             };
//             // let task = Task::new(i, Box::new(future));
//             Executor::spawn(future);
//         }
//         executor.block();
//     }

//     #[test]
//     fn test3() {
//         let executor = Executor::new(10);
//         for i in 0..1000 {
//             let future = async move {
//                 // println!("start:{:?}", NOW.elapsed());
//                 let thread = thread::current();
//                 let stream =
//                     TcpStream::connect("127.0.0.1:3000").await.unwrap();
//                 let mut buf = [0u8; 10];
//                 buf[0] = i as u8;
//                 stream.write(&buf).await.unwrap();
//                 stream.read(&mut buf).await.unwrap();
//                 println!(
//                     "{:?} done{}[{:?}]:{:?}",
//                     thread.name().unwrap(),
//                     i,
//                     NOW.elapsed(),
//                     buf
//                 );
//             };
//             // let task = Task::new(i, Box::new(future));
//             Executor::spawn(future);
//         }
//         // executor.block();
//         thread::sleep(Duration::from_millis(1000));
//     }
// }
