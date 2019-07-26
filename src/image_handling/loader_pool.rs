use std::path::PathBuf;
use std::sync::mpsc::Sender;
use crate::image::ImageData;
use crate::worker_pool::{WorkerPool, Worker};
use glium::glutin::event_loop::{EventLoop, EventLoopProxy, EventLoopClosed};

  // using separate channels to notify about load, and actually send the load,
  // because the payload for winit user events is constrained to be Clone, which is not what I want.
#[derive(Debug)]
pub enum LoadNotification {
  ImageLoaded,
  LoadFailed
}

pub struct LoadWorker {
  id: usize,
  event_loop_proxy: EventLoopProxy<LoadNotification>,
}

impl Worker for LoadWorker {
  type Input = (PathBuf, usize);
  type Output = (ImageData, usize);

  fn execute(&mut self, input: Self::Input, output: &Sender<Self::Output>) {
    let (path, idx) = input;
    let img_data_res = ImageData::load(&path);
    let event_message = 
      if let Ok(img_data) = img_data_res {
        let output_data = (img_data, idx);
        let send_res = output.send(output_data);
        match send_res {
          Ok(_) => {
            println!("Worker {}: channel send succeeded", self.id);
            LoadNotification::ImageLoaded
          },
          Err(error) => {
            println!("Worker {}: channel send failed, {}", self.id, error);
            LoadNotification::LoadFailed
          }
        }
      } else {
        LoadNotification::LoadFailed
      };

    match self.event_loop_proxy.send_event(event_message) {
      Ok(()) => println!("Worker {}: Send succeeded", self.id),
      Err(EventLoopClosed) => println!("Worker {}: Event loop closed", self.id)
    };
  }
}

pub type LoaderPool = WorkerPool<LoadWorker>;
pub fn new(size: usize, event_loop: &EventLoop<LoadNotification>)->LoaderPool {
  WorkerPool::new(size, |id| {
    LoadWorker {
      id: id,
      event_loop_proxy: event_loop.create_proxy()
    }
  })
}