use std::error::Error;
use std::io;
use std::fmt;
use std::path::Path;
use std::fs::{self, DirEntry};
use glium::backend::Facade;
use crate::image::{self, ImageData, PlacedImage};

  // A loaded directory of images we want to display
pub struct LoadedDir {
  pub path: Box<Path>,
  pub entries: Vec<DirEntry>,
  pub shown_idx: usize,
  pub shown_image: PlacedImage
}

impl LoadedDir {
  pub fn new<F: Facade>(path: &Path, gl_ctx: &F)->Result<LoadedDir, DirLoadError> {
    if !path.is_dir() {
      return Err(DirLoadError::NotADirectory);
    }

    let path = path.to_path_buf().into_boxed_path();
    let dir_iter = fs::read_dir(&path)?;

    let entries: Vec<_> = dir_iter
      .filter_map(|entry_res| entry_res.ok())
      .filter(|entry| is_relevant_file(entry))
      .collect();

    let shown_idx = 0;
    let img_path = entries[shown_idx].path();
    let image_data = ImageData::load(&img_path, gl_ctx)?;

    let shown_image = PlacedImage::new(image_data);

    Ok(LoadedDir {
      path,
      entries,
      shown_idx,
      shown_image
    })
  }

  pub fn change_image<F: Facade>(&mut self, offset: i32, gl_ctx: &F)->Result<(), image::ImageLoadError> {
    let mut signed_idx = self.shown_idx as i32;
    let entries_len = self.entries.len() as i32;

      // wrapping offset
    signed_idx += offset;
    while signed_idx < 0 {
      signed_idx += entries_len;
    }
    while signed_idx >= entries_len {
      signed_idx -= entries_len;
    }
    let usize_idx = signed_idx as usize;
    let img_path = self.entries[usize_idx].path();

    let image_data = ImageData::load(&img_path, gl_ctx)?;
    self.shown_image = PlacedImage::new(image_data);
    self.shown_idx = usize_idx; // only assign new index once loading the new data succeeded

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