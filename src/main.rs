use std::{thread, time::Duration};

use ay_runtime::{runtime::executor::Executor, tcp::{Sleep, SleepFd, TcpStream}, utils::NOW};
use proc_macro::ay_main;

#[ay_main(worker_threads = 10)]
async fn main() {
    for i in 0..1000 {
        let future = async move {
            let thread = thread::current();
            // let stream = TcpStream::connect("127.0.0.1:3000").await.unwrap();
            let mut buf = Vec::<u8>::new();
            for i in 0..10 {
                buf.push(i);
            }
            SleepFd::new(1).await;
            // Sleep::new(Duration::from_millis(1)).await;
            // stream.write(&buf).await.unwrap();
            // stream.read(&mut buf).await.unwrap();
            // println!(
            //     "{} {}done:{}",
            //     thread.name().unwrap(),
            //     i,
            //     NOW.elapsed().as_millis()
            // );
        };
        Executor::spawn(future);
    }
    thread::sleep(Duration::new(1, 0));
}
