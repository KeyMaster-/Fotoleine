use std::error::Error;
use std::io;
use std::fmt;
use std::path::{Path, PathBuf};
use std::fs::{self, DirEntry};
use glium::backend::Facade;
use glium::texture::TextureCreationError;
use crate::image::{self, ImageData, ImageTexture, PlacedImage};
use crate::LoaderPool;

  // A loaded directory of images we want to display
pub struct LoadedDir {
  path: Box<Path>,
  entries: Vec<DirEntry>,
  loaded_images: Vec<Option<PlacedImage>>,
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
      .filter(|entry| is_relevant_file(entry))
      .collect();

    let mut loaded_images = Vec::with_capacity(entries.len());
    loaded_images.resize_with(entries.len(), Default::default);

    let shown_idx = 0;

    Ok(LoadedDir {
      path,
      entries,
      loaded_images,
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

  pub fn set_shown(&mut self, idx: usize, loader_pool: &LoaderPool) {
    self.shown_idx = idx;
    if self.loaded_images[idx].is_none() {
      self.submit_load_request(idx, &loader_pool);
    }

    let prev_idx = offset_idx(self.shown_idx, self.entries.len(), -1);
    if self.loaded_images[prev_idx].is_none() {
      self.submit_load_request(prev_idx, &loader_pool);
    }

    let next_idx = offset_idx(self.shown_idx, self.entries.len(), 1);
    if self.loaded_images[next_idx].is_none() {
      self.submit_load_request(next_idx, &loader_pool);
    }
  }

  fn submit_load_request(&self, idx: usize, loader_pool: &LoaderPool) {
    let path = self.entries[idx].path();
    loader_pool.submit((path, idx));
  }

  pub fn process_loaded_image<F: Facade>(&mut self, load_output: (ImageData, usize), gl_ctx: &F)->Result<(), TextureCreationError> {
    let (image_data, idx) = load_output;
    let texture = ImageTexture::from_data(image_data, gl_ctx)?;
    let placed_image = PlacedImage::new(texture);

    if self.loaded_images[idx].is_none() {
      self.loaded_images[idx] = Some(placed_image);
    } else {
      println!("Image {} was already loaded!", idx);
    };

    Ok(())
  }
}

fn is_relevant_file(entry:&DirEntry)->bool {
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