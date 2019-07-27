use std::path::Path;
use loader_pool::{LoaderPool, LoadNotification};
use loaded_dir::{LoadedDir, DirLoadError};
use glium::glutin::event_loop::EventLoop;

mod loaded_dir;
pub mod loader_pool;

pub struct ImageHandling {
  pub services: ImageHandlingServices,
  pub loaded_dir: Option<LoadedDir>
}

impl ImageHandling {
  pub fn new(load_behind_count: usize, load_ahead_count: usize, thread_pool_size: usize, event_loop: &EventLoop<LoadNotification>)->ImageHandling {
    let services = ImageHandlingServices::new(load_behind_count, load_ahead_count, thread_pool_size, event_loop);
    ImageHandling {
      services,
      loaded_dir: None
    }
  }

  pub fn load_path(&mut self, path: &Path)->Result<(), DirLoadError> {
    let loaded_dir = LoadedDir::new(path, &self.services)?;
    self.loaded_dir = Some(loaded_dir);
    Ok(())
  }
}

pub struct ImageHandlingServices {
  loader_pool: LoaderPool,
  load_behind_count: usize,
  load_ahead_count: usize
}

impl ImageHandlingServices {
  fn new(load_behind_count: usize, load_ahead_count: usize, thread_pool_size: usize, event_loop: &EventLoop<LoadNotification>)->ImageHandlingServices {
    let loader_pool = loader_pool::new(thread_pool_size, event_loop);
    ImageHandlingServices {
      load_behind_count,
      load_ahead_count,
      loader_pool
    }
  }
}