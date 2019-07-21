use std::sync::mpsc::{Sender, Receiver, channel};
use std::thread::{self, JoinHandle};
pub struct WorkerPool<W: Worker + 'static + Send> {
  worker_threads: Vec<Option<JoinHandle<()>>>,
  pub output: Receiver<W::Output>
}

pub trait Worker {
  type Input: 'static + Send;
  type Output: 'static + Send;

  fn execute(&mut self, input: Self::Input, output: Sender<Self::Output>);
}

impl<W: Worker + Send> WorkerPool<W> where W::Input: Clone {
  pub fn new<F>(n_workers: usize, spawn_worker: F, dummy_input: W::Input)->WorkerPool<W>
    where F: Fn(usize)->W {

    assert!(n_workers > 0);

    let (output_tx, output_rx) = channel();

    let worker_threads: Vec<_> = (0..n_workers).map(|id| {
        let mut worker = spawn_worker(id);
        let output = output_tx.clone();
        let input_clone = dummy_input.clone();
        Some(thread::spawn(move || {
          worker.execute(input_clone, output);
        }))
      }).collect();

    WorkerPool {
      worker_threads,
      output: output_rx
    }
  }
}

impl<W: Worker + 'static + Send> Drop for WorkerPool<W> {
  fn drop(&mut self) {
    for handle in &mut self.worker_threads {
      if let Some(handle) = handle.take() {
        handle.join().expect("Couldn't join on thread.");
      }
    }
  }
}

// worker pool spawns n workers using factory closure
// each created worker stores an event loop proxy

// worker api:
  // execute(input: Self::Input, output: Sender<Self::Output>)
  // get some input (in this specific case, an image id and path), then execute your work and write output to the sender
  // in this case, that'll go:
    // load image
    // load exif
    // package up image bytes and exif into tuple, send along output channel
    // send user event to proxy to notify about load result (will trigger try_recv loop in main thread, processing of images, and subsequent redraw)


// worker pool surrounding infrastructure
  // for each worker, spawn a thread with an internal closure
  // the closure
    // gets to own the worker
    // also gets a reference to the job distribution channel (receiving end, in Arc<Mutex<>>)
    // also gets a reference to the sender end of the results channel (gets passed to worker execute function to write output)

  // the closure will 
    // wait to get access to the job distribution channel (blocking acquiring of the mutex)
    // wait to get a job (blocking recv on the job distribution channel)
    // once a job has arrived, run the worker execute function
      // passing in the input, as well as its end of the output channel

  // worker pool offers the output channel in its api, the main channel can try_recv on it
  
  // job distribution channel has to be typed on enum, with variants being JobInput and Terminate
  // on drop of worker pool, send out terminate signals to all thread closures, then join on all