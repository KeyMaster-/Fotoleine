use std::path::Path;
use std::ops::RangeInclusive;
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
  pub fn new(buffer_zone_count: usize, load_behind_count: usize, load_ahead_count: usize, thread_pool_size: usize, event_loop: &EventLoop<LoadNotification>)->ImageHandling {
    let services = ImageHandlingServices::new(buffer_zone_count, load_behind_count, load_ahead_count, thread_pool_size, event_loop);
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
  loading_policy: ImageLoadingPolicy 
}

impl ImageHandlingServices {
  fn new(buffer_zone_count: usize, load_behind_count: usize, load_ahead_count: usize, thread_pool_size: usize, event_loop: &EventLoop<LoadNotification>)->ImageHandlingServices {
    let loader_pool = loader_pool::new(thread_pool_size, event_loop);
    let loading_policy = ImageLoadingPolicy::new(buffer_zone_count, load_behind_count, load_ahead_count);
    ImageHandlingServices {
      loader_pool,
      loading_policy
    }
  }
}

struct ImageLoadingPolicy {
  buffer_zone_count: usize, // how many images ahead and behind you can move around before triggering new loads // :todo: naming.
  load_behind_count: usize,
  load_ahead_count: usize
}

impl ImageLoadingPolicy {
  fn new(buffer_zone_count: usize, load_behind_count: usize, load_ahead_count: usize)->ImageLoadingPolicy {
    ImageLoadingPolicy {
      buffer_zone_count,
      load_behind_count,
      load_ahead_count
    }
  }

  pub fn max_loaded_image_count(&self)->usize {
    return 1 + self.buffer_zone_count * 2 + self.load_behind_count + self.load_ahead_count;
  }

  pub fn get_load_range(&self, pivot: usize, shown_idx: usize, max: usize)->(usize, RangeInclusive<usize>) { // new pivot, load range
    if self.buffer_zone_range(pivot).contains(&(shown_idx as i32)) {
      (pivot, self.range_around_pivot(pivot, max))
    } else {
      (shown_idx, self.range_around_pivot(shown_idx, max))
    }
  }

  fn buffer_zone_range(&self, pivot: usize)->RangeInclusive<i32> {
    let start = (pivot as i32) - (self.buffer_zone_count as i32);
    let end = (pivot as i32) + (self.buffer_zone_count as i32);

    start..=end
  }

  fn range_around_pivot(&self, pivot: usize, max: usize)->RangeInclusive<usize> {
    let start = (pivot as i32) - (self.buffer_zone_count as i32) - (self.load_behind_count as i32);
    let end = (pivot as i32) + (self.buffer_zone_count as i32) + (self.load_ahead_count as i32);

    let start = clamp(start, 0, (max - 1) as i32) as usize;
    let end = clamp(end, 0, (max - 1) as i32) as usize;

    start..=end
  }
}

  // clamps v in [mi, ma]
fn clamp(v: i32, mi: i32, ma: i32)->i32 {
  v.max(mi).min(ma)
}