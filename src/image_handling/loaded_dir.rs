use std::error::Error;
use std::io;
use std::fmt;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::fs::{self, DirEntry};
use std::collections::{HashMap, HashSet};
use glium::backend::Facade;
use glium::texture::TextureCreationError;
use crate::image::{self, ImageTexture, PlacedImage};
use super::ImageHandlingServices;


  // A loaded directory of images we want to display
pub struct LoadedDir {
  path: Box<Path>,
  entries: Vec<DirEntry>,

  loaded_images: HashMap<usize, PlacedImage>,
  shown_idx: usize,

  pending_loads: HashSet<usize>,
}

fn offset_idx(idx: usize, max: usize, offset: i32)->usize {
  let mut signed_idx = idx as i32;
  let max = max as i32;

  signed_idx += offset;

  signed_idx.max(0).min(max - 1) as usize // clamp to [0, max-1]
}

impl LoadedDir {
  pub fn new(path: &Path, services: &ImageHandlingServices)->Result<LoadedDir, DirLoadError> {
    if !path.is_dir() {
      return Err(DirLoadError::NotADirectory);
    }

    let path = path.to_path_buf().into_boxed_path();
    let dir_iter = fs::read_dir(&path)?;

    let entries: Vec<_> = dir_iter
      .filter_map(|entry_res| entry_res.ok())
      .filter(|entry| file_is_relevant(entry))
      .collect();

    let loaded_images = HashMap::with_capacity(1 + services.load_behind_count + services.load_ahead_count);
    let pending_loads = HashSet::new();

    let shown_idx = 0;

    Ok(LoadedDir {
      path,
      entries,
      loaded_images,
      shown_idx,
      pending_loads,
    })
  }

  pub fn shown_idx(&self)->usize {
    self.shown_idx
  }

  pub fn offset_idx(&self, offset: i32)->usize {
    offset_idx(self.shown_idx, self.entries.len(), offset)
  }

  pub fn image_at(&self, idx: usize)->Option<&PlacedImage> {
    self.loaded_images.get(&idx)
  }

  pub fn image_at_mut(&mut self, idx: usize)->Option<&mut PlacedImage> {
    self.loaded_images.get_mut(&idx)
  }

  pub fn path_at(&self, idx: usize)->PathBuf {
    self.entries[idx].path()
  }

  fn idxs_to_load(&self, idx: usize, services: &ImageHandlingServices)->RangeInclusive<usize> {
    let start = offset_idx(idx, self.entries.len(), -(services.load_behind_count as i32));
    let end = offset_idx(idx, self.entries.len(), services.load_ahead_count as i32);

    start..=end
  }

  pub fn set_shown(&mut self, idx: usize, services: &ImageHandlingServices) {
    self.shown_idx = idx;

    let to_load = self.idxs_to_load(idx, services);
    self.loaded_images.retain(|key, _| to_load.contains(key));

    for idx in to_load {
      if self.needs_load(idx) {
        self.submit_load_request(idx, services);
      }
    }
  }

  fn needs_load(&self, idx: usize)->bool {
    !self.loaded_images.contains_key(&idx) && !self.pending_loads.contains(&idx)
  }

  fn submit_load_request(&mut self, idx: usize, services: &ImageHandlingServices) {
    let path = self.entries[idx].path();
    self.pending_loads.insert(idx);
    services.loader_pool.submit((path, idx));
  }

  pub fn receive_image<F: Facade>(&mut self, services: &ImageHandlingServices, gl_ctx: &F)->Result<(), TextureCreationError> {
    let load_output_res = services.loader_pool.output.recv(); // :todo: pass error to outside
    if let Ok(load_output) = load_output_res {
      let (image_data, idx) = load_output;

      if !self.loaded_images.contains_key(&idx) {

        let texture = ImageTexture::from_data(image_data, gl_ctx)?;
        let placed_image = PlacedImage::new(texture);

        self.loaded_images.insert(idx, placed_image);
        if !self.pending_loads.remove(&idx) {
          println!("Loaded {}, but no corresponding pending load existed.", idx);
        }
      } else {
        println!("Image {} was already loaded!", idx);
      };

      Ok(())
    } else {
      println!("loader pool output channel closed!");
      Ok(())
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