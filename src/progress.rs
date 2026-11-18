use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct Activity {
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Activity {
    pub fn start(label: &'static str) -> Self {
        let (stop, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let start = Instant::now();
            while receiver.recv_timeout(Duration::from_secs(3)).is_err() {
                println!("  {label}... прошло {} сек.", start.elapsed().as_secs());
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
