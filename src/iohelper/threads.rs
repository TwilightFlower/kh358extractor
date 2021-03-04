use std::{
	sync::{
		Arc,
		atomic::{AtomicU32, AtomicBool, Ordering}
	},
	thread::{JoinHandle, spawn, sleep},
	time::Duration,
	mem::replace
};
use crossbeam_channel::{
	Sender, Receiver, unbounded, SendError
};

pub struct JankyThreadPool<T: Send + 'static> {
	pending_task_count: Arc<AtomicU32>,
	live: Arc<AtomicBool>,
	tx: Sender<T>,
	rx: Receiver<T>,
	threads: Vec<JoinHandle<()>>
}

#[derive(Clone)]
pub struct TaskSender<T: Send + 'static> {
	pending_task_count: Arc<AtomicU32>,
	tx: Sender<T>
}

impl<T: Send + 'static> TaskSender<T> {
	pub fn send(&self, task: T) -> Result<(), SendError<T>> {
		self.pending_task_count.fetch_add(1, Ordering::Relaxed);
		self.tx.send(task)
	}
}

impl<T: Send + 'static> JankyThreadPool<T> {
	pub fn new<U: Send + 'static>(thread_count: u32, task_consumer: impl Fn(T, &U) + Send + 'static + Clone, wrapper: impl Fn(TaskSender<T>) -> U) -> Self {
		let (tx, rx) = unbounded();
		let mut pool = JankyThreadPool {
			tx, rx,
			pending_task_count: Arc::new(AtomicU32::new(0)),
			live: Arc::new(AtomicBool::new(true)),
			threads: Vec::new(),
		};
		for _ in 0..thread_count {
			let (pending_task_count, live, tx, rx, tc) = (pool.pending_task_count.clone(), pool.live.clone(), pool.tx.clone(), pool.rx.clone(), task_consumer.clone());
			let tx = wrapper(TaskSender {
				pending_task_count: pending_task_count.clone(), tx
			});
			pool.threads.push(spawn(move || {
				while live.load(Ordering::Relaxed) || pending_task_count.load(Ordering::Relaxed) > 0 {
					if let Ok(t) = rx.recv_timeout(Duration::new(5, 0)) {
						tc(t, &tx);
						pending_task_count.fetch_sub(1, Ordering::Relaxed);
					}
				}
			}))
		}
		pool
	}
	
	pub fn task_sender(&self) -> TaskSender<T> {
		TaskSender {
			tx: self.tx.clone(),
			pending_task_count: self.pending_task_count.clone()
		}
	}
	
	pub fn wait_for_tasks(&self) {
		while self.pending_task_count.load(Ordering::Relaxed) > 0 {
			sleep(Duration::new(5, 0))
		}
	}
	
	pub fn shutdown(&self) {
		self.live.store(false, Ordering::Relaxed);
	}
	
	pub fn join(mut self) {
		self.shutdown();
		for t in replace(&mut self.threads, Vec::new()) {
			t.join();
		}
	}
}

impl<T: Send + 'static> Drop for JankyThreadPool<T> {
	fn drop(&mut self) {
		self.shutdown()
	}
}
