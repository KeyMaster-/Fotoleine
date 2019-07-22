use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
pub struct WorkerPool<W: Worker + 'static + Send> {
  pub output: Receiver<W::Output>,
  worker_threads: Vec<Option<JoinHandle<()>>>,
  task_sender: Sender<TaskMessage<W>>
}

pub trait Worker {
  type Input: 'static + Send;
  type Output: 'static + Send;

  fn execute(&mut self, input: Self::Input, output: &Sender<Self::Output>);
}

enum TaskMessage<W: Worker> {
  NewTask(W::Input),
  Terminate
}

impl<W: Worker + 'static + Send> WorkerPool<W> {
  pub fn new<F>(n_workers: usize, spawn_worker: F)->WorkerPool<W>
    where F: Fn(usize)->W {

    assert!(n_workers > 0);

    let (task_tx, task_rx) = channel();
    let base_task_receiver = Arc::new(Mutex::new(task_rx));

    let (output_tx, output_rx) = channel();

    let worker_threads: Vec<_> = (0..n_workers).map(|id| {
        let mut worker = spawn_worker(id);
        let output = output_tx.clone();
        let task_receiver = Arc::clone(&base_task_receiver);

        Some(thread::spawn(move || {
          loop {
            let task_message = task_receiver.lock().expect("Error when locking the job mutex").recv().expect("Error when getting new job."); //:todo: error handling

            match task_message {
              TaskMessage::NewTask(input) => {
                thread::sleep(std::time::Duration::from_millis(1000));
                worker.execute(input, &output);
              },
              TaskMessage::Terminate => {
                break;
              }
            }
          }
        }))
      }).collect();

    WorkerPool {
      output: output_rx,
      worker_threads,
      task_sender: task_tx
    }
  }

  pub fn submit(&self, input: W::Input) {
    self.task_sender.send(TaskMessage::NewTask(input)).expect("Couldn't send input task.");
  }
}

impl<W: Worker + 'static + Send> Drop for WorkerPool<W> {
  fn drop(&mut self) {
    println!("Notifying all workers of termination");

    for _ in &mut self.worker_threads {
      self.task_sender.send(TaskMessage::Terminate).expect("Couldn't send terminate to worker");
    }

    println!("Joining on all workers");

    for handle in &mut self.worker_threads {
      if let Some(handle) = handle.take() {
        handle.join().expect("Couldn't join on thread.");
      }
    }
  }
}