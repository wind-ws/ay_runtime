use std::thread;

use ay_runtime::{runtime::executor::Executor, tcp::TcpStream, utils::NOW};
use proc_macro::ay_main;

#[ay_main(worker_threads = 10)]
async fn main() {
    for i in 0..1000 {
        let future = async move {
            let thread = thread::current();
            let stream = TcpStream::connect("127.0.0.1:3000").await.unwrap();
            let mut buf = [0u8; 10];
            stream.write(&buf).await.unwrap();
            stream.read(&mut buf).await.unwrap();
            println!(
                "{} done{}[{:?}]:{:?}",
                thread.name().unwrap(),
                i,
                NOW.elapsed(),
                buf
            );
        };
        Executor::spawn(future);
    }
}

// 对应的服务端代码
// #[tokio::main(flavor = "multi_thread", worker_threads = 10)]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     let listener = TcpListener::bind("127.0.0.1:3000").await?;
//     let mut count = 0u64;
//     loop {
//         let (mut socket, _) = listener.accept().await?;
//         count+=1;
//         println!("{}",count);
//         tokio::spawn(async move {
//             let mut buf = [0; 10];

//                 let n = match socket.read(&mut buf).await {
//                     Ok(0) => return,
//                     Ok(n) => n,
//                     Err(e) => {
//                         eprintln!("failed to read from socket; err = {:?}", e);
//                         return;
//                     }
//                 };
//                 buf[0]= count as u8;
//                 if let Err(e) = socket.write_all(&buf[0..n]).await {
//                     eprintln!("failed to write to socket; err = {:?}", e);
//                     return;
//                 }

//         });
//     }
// }

// #[ay_test(worker_threads = 10)]
// async fn test() {
//     for i in 0..1000 {
//         let future = async move {
//             let thread = thread::current();
//             let stream = TcpStream::connect("127.0.0.1:3000").await.unwrap();
//             let mut buf = [0u8; 10];
//             stream.write(&buf).await.unwrap();
//             stream.read(&mut buf).await.unwrap();
//             println!(
//                 "{} done{}[{:?}]:{:?}",
//                 thread.name().unwrap(),
//                 i,
//                 NOW.elapsed(),
//                 buf
//             );
//         };
//         Executor::spawn(future);
//     }
// }
