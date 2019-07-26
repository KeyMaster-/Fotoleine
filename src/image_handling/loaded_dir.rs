use std::error::Error;
use std::io;
use std::fmt;
use std::path::{Path, PathBuf};
use std::fs::{self, DirEntry};
use std::collections::VecDeque;
use glium::backend::Facade;
use glium::texture::TextureCreationError;
use crate::image::{self, ImageTexture, PlacedImage};
use super::ImageHandlingServices;


  // A loaded directory of images we want to display
pub struct LoadedDir {
  path: Box<Path>,
  entries: Vec<DirEntry>,
  loaded_images: Vec<Option<PlacedImage>>,
  load_history: VecDeque<usize>, // older loads at the front, newer loads at the back
  
  shown_idx: usize,
}

fn offset_idx(idx: usize, max: usize, offset: i32)->usize {
  let mut signed_idx = idx as i32;
  let max = max as i32;

  signed_idx += offset;
  while signed_idx < 0 {
    signed_idx += max;
  }
  while signed_idx >= max {
    signed_idx -= max;
  }
  signed_idx as usize
}

impl LoadedDir {
  pub fn new(path: &Path)->Result<LoadedDir, DirLoadError> {
    if !path.is_dir() {
      return Err(DirLoadError::NotADirectory);
    }

    let path = path.to_path_buf().into_boxed_path();
    let dir_iter = fs::read_dir(&path)?;

    let entries: Vec<_> = dir_iter
      .filter_map(|entry_res| entry_res.ok())
      .filter(|entry| file_is_relevant(entry))
      .collect();

    let mut loaded_images = Vec::with_capacity(entries.len());
    loaded_images.resize_with(entries.len(), Default::default);
    let load_history = VecDeque::new();

    let shown_idx = 0;

    Ok(LoadedDir {
      path,
      entries,
      loaded_images,
      load_history,
      shown_idx,
    })
  }

  pub fn shown_idx(&self)->usize {
    self.shown_idx
  }

  pub fn offset_idx(&self, offset: i32)->usize {
    offset_idx(self.shown_idx, self.entries.len(), offset)
  }

  pub fn image_at(&self, idx: usize)->&Option<PlacedImage> {
    &self.loaded_images[idx]
  }

  pub fn image_at_mut(&mut self, idx: usize)->&mut Option<PlacedImage> {
    &mut self.loaded_images[idx]
  }

  pub fn path_at(&self, idx: usize)->PathBuf {
    self.entries[idx].path()
  }

  pub fn set_shown(&mut self, idx: usize, services: &ImageHandlingServices) {
    self.shown_idx = idx;
    if self.loaded_images[idx].is_none() {
      self.submit_load_request(idx, services);
    }

    let prev_idx = offset_idx(self.shown_idx, self.entries.len(), -1);
    if self.loaded_images[prev_idx].is_none() {
      self.submit_load_request(prev_idx, services);
    }

    let next_idx = offset_idx(self.shown_idx, self.entries.len(), 1);
    if self.loaded_images[next_idx].is_none() {
      self.submit_load_request(next_idx, services);
    }
  }

  fn submit_load_request(&self, idx: usize, services: &ImageHandlingServices) {
    let path = self.entries[idx].path();
    services.loader_pool.submit((path, idx));
  }

  pub fn receive_image<F: Facade>(&mut self, services: &ImageHandlingServices, gl_ctx: &F)->Result<(), TextureCreationError> {
    let load_output_res = services.loader_pool.output.recv(); // :todo: pass to outside
    if let Ok(load_output) = load_output_res {
      let (image_data, idx) = load_output;

      if self.loaded_images[idx].is_none() {
        self.unload_to_free(1, services);
        let texture = ImageTexture::from_data(image_data, gl_ctx)?;
        let placed_image = PlacedImage::new(texture);
        self.loaded_images[idx] = Some(placed_image);
        self.load_history.push_back(idx);
      } else {
        println!("Image {} was already loaded!", idx);
      };

      Ok(())
    } else {
      println!("loader pool output channel closed!");
      Ok(())
    }
  }

  fn unload_to_free(&mut self, free_target: usize, services: &ImageHandlingServices) {
    while self.load_history.len() > services.max_images_loaded - free_target {
      if let Some(unload_idx) = self.load_history.pop_front() {
        self.loaded_images[unload_idx].take();
      } else {
        println!("Needed to unload images, but no indices exist to unload?");
      }
    }
  }
}

fn file_is_relevant(entry:&DirEntry)->bool {
  let path = entry.path();
  if !path.is_file() {
    return false;
  }

  let ext_str = path.extension().and_then(|ext| ext.to_str());

  if ext_str.is_none() { // no extension, or no unicode extension
    return false;
  }
  let ext_lowercase = ext_str.unwrap().to_lowercase();
  let ext_matches = ext_lowercase == "jpg" || ext_lowercase == "jpeg";

  let stem_str = path.file_stem().and_then(|stem| stem.to_str());
  if stem_str.is_none() { // no stem, or no unicode stem
    return false;
  }
  let stem_okay = !stem_str.unwrap().starts_with("._");

  ext_matches && stem_okay
}

  // :todo: consider using snafu, io error has specific context of being during entry reading
  // issue is easy From trait implementations for use in ImageData::load
#[derive(Debug)]
pub enum DirLoadError {
  NotADirectory,
  IoError(io::Error),
  ImageLoadError(image::ImageLoadError)
}

impl fmt::Display for DirLoadError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::DirLoadError::*;
    match self {
      NotADirectory => write!(f, "Given path is not a directory"),
      IoError(error) => write!(f, "Could not read directory entries: {}", error),
      ImageLoadError(error) => write!(f, "Could not load initial image: {}", error),
    }
  }
}

impl Error for DirLoadError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::DirLoadError::*;
    match self {
      NotADirectory => None,
      IoError(error) => Some(error),
      ImageLoadError(error) => Some(error)
    }
  }
}

impl From<io::Error> for DirLoadError {
  fn from(error: io::Error)->Self {
    DirLoadError::IoError(error)
  }
}

impl From<image::ImageLoadError> for DirLoadError {
  fn from(error: image::ImageLoadError)->Self {
    DirLoadError::ImageLoadError(error)
  }
}