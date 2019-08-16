use std::error::Error;
use std::io::{self, Write};
use std::fmt;
use std::path::{Path, PathBuf};
use std::fs::{self, File, DirEntry};
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
  load_pivot: usize,
  shown_idx: usize,

  pending_loads: HashSet<usize>,

  ratings: ImageRatings
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

    let mut entries: Vec<_> = dir_iter
      .filter_map(|entry_res| entry_res.ok())
      .filter(|entry| file_is_relevant(entry))
      .collect();

    entries.sort_unstable_by_key(|entry| entry.file_name());

    let loaded_images = HashMap::with_capacity(services.loading_policy.max_loaded_image_count());
    let pending_loads = HashSet::new();

    let shown_idx = 0;
    let load_pivot = 0;

    let ratings = ImageRatings::new(&path)?;

    let mut loaded_dir = LoadedDir {
      path,
      entries,
      loaded_images,
      load_pivot,
      shown_idx,
      pending_loads,
      ratings,
    };

    loaded_dir.update_loaded(services);

    Ok(loaded_dir)
  }

  pub fn shown_idx(&self)->usize {
    self.shown_idx
  }

  pub fn image_count(&self)->usize {
    self.entries.len()
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

  pub fn set_shown(&mut self, idx: usize, services: &ImageHandlingServices) {
    self.shown_idx = idx;

    self.update_loaded(services);
  }

  fn file_name_string(&self, idx: usize)->String {
    self.entries[idx].file_name().into_string().unwrap() // the image filter removes any entries which don't have a rust-string-representable filename
  }

  pub fn set_rating(&mut self, idx: usize, rating: Rating) {
    let file_name = self.file_name_string(idx);
    let save_res = self.ratings.set_rating(file_name, rating);
    if let Err(error) = save_res {
      println!("Failed to save ratings: {}", error);
    }
  }

  pub fn get_rating(&self, idx: usize)->Rating {
    let file_name = self.file_name_string(idx);
    self.ratings.get_rating(&file_name)
  }

  fn update_loaded(&mut self, services: &ImageHandlingServices) {
    let (new_pivot, load_range) = services.loading_policy.get_load_range(self.load_pivot, self.shown_idx, self.entries.len());
    self.load_pivot = new_pivot;

    self.loaded_images.retain(|key, _| load_range.contains(key));

    for idx in load_range {
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

    // The filename is not representable as a rust string
    // This is required for saving image ratings
  if entry.file_name().into_string().is_err() {
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
  RatingsLoadError(RatingsLoadError)
  // ImageLoadError(image::ImageLoadError),
}

impl fmt::Display for DirLoadError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::DirLoadError::*;
    match self {
      NotADirectory => write!(f, "Given path is not a directory"),
      IoError(error) => write!(f, "Could not read directory entries: {}", error),
      RatingsLoadError(error) => write!(f, "Could not load the ratings file: {}", error)
      // ImageLoadError(error) => write!(f, "Could not load initial image: {}", error),
    }
  }
}

impl Error for DirLoadError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::DirLoadError::*;
    match self {
      NotADirectory => None,
      IoError(error) => Some(error),
      RatingsLoadError(error) => Some(error),
      // ImageLoadError(error) => Some(error)
    }
  }
}

impl From<io::Error> for DirLoadError {
  fn from(error: io::Error)->Self {
    DirLoadError::IoError(error)
  }
}

impl From<RatingsLoadError> for DirLoadError {
  fn from(error: RatingsLoadError)->Self {
    DirLoadError::RatingsLoadError(error)
  }
}

// impl From<image::ImageLoadError> for DirLoadError {
//   fn from(error: image::ImageLoadError)->Self {
//     DirLoadError::ImageLoadError(error)
//   }
// }

struct ImageRatings {
  ratings: HashMap<String, Rating>,
  folder_path: PathBuf,
  ratings_file_path: PathBuf,
}

impl ImageRatings {
  fn new(folder_path: &Path)->Result<ImageRatings, RatingsLoadError> {
    let folder_path = folder_path.to_path_buf();

    let mut ratings_file_path = folder_path.clone();
    ratings_file_path.push("ratings.yaml");

    let ratings = ImageRatings::load_ratings(&ratings_file_path)?;

    Ok(ImageRatings {
      ratings,
      folder_path,
      ratings_file_path,
    })
  }

  fn load_ratings(file_path: &Path)->Result<HashMap<String, Rating>, RatingsLoadError> {
    if !file_path.exists() {
      return Ok(HashMap::new());
    }

    if file_path.is_dir() {
      return Err(RatingsLoadError::PathIsDir);
    }

    let file = File::open(file_path)?;
    let mut deser_map: HashMap<String, u8> = serde_yaml::from_reader(file)?;
    let mut ratings_map = HashMap::with_capacity(deser_map.len());
    for (k, v) in deser_map.drain() {
      ratings_map.insert(k, Rating::from_u8(v));
    }

    Ok(ratings_map)
  }

  fn set_rating(&mut self, img_name: String, rating: Rating)->Result<(), RatingsSaveError> {
    if let Rating::Low = rating {
      self.ratings.remove(&img_name);
    } else {
      self.ratings.insert(img_name, rating);
    }

    self.save_ratings()
  }

  fn get_rating(&self, img_name: &String)->Rating {
    if let Some(rating) = self.ratings.get(img_name) {
      *rating
    } else {
      Rating::Low
    }
  }

  fn save_ratings(&self)->Result<(), RatingsSaveError> {
    let ratings_ser = RatingsSerialize { ratings: &self.ratings };

    let s = serde_yaml::to_string(&ratings_ser)?;

    let mut tmp_file = tempfile::NamedTempFile::new_in(&self.folder_path)?;
    tmp_file.as_file_mut().write(s.as_bytes())?;
    tmp_file.persist(&self.ratings_file_path)?;

    
    Ok(())
  }
}

#[derive(Debug, Copy, Clone)]
pub enum Rating {
  Low,
  Medium,
  High,
}

impl Rating {
  pub fn from_u8(val: u8)->Rating {
    let limited = val.min(2); // limit to [0, 2] range
    if limited == 0 {
      Rating::Low
    } else if limited == 1 {
      Rating::Medium
    } else {
      Rating::High
    }
  }

  pub fn to_u8(&self)->u8 {
    match self {
      Rating::Low => 0,
      Rating::Medium => 1,
      Rating::High => 2
    }
  }

  pub fn max()->u8 { return 2; }
}

struct RatingsSerialize<'a> {
  ratings: &'a HashMap<String, Rating>
}

use serde::ser::{Serialize, Serializer, SerializeMap};

impl<'a> Serialize for RatingsSerialize<'a> {
  fn serialize<S>(&self, serializer: S)->Result<S::Ok, S::Error>
    where S: Serializer
  {
    let mut entries: Vec<_> = self.ratings.iter().collect();
    entries.sort_unstable_by_key(|kv| kv.0);

    let mut map = serializer.serialize_map(Some(entries.len()))?;
    for (path, rating) in entries {
      let rating = rating.to_u8();
      map.serialize_entry(path, &rating)?;
    }
    map.end()
  }
}

#[derive(Debug)]
pub enum RatingsSaveError {
  SerializeError(serde_yaml::Error),
  WriteError(io::Error),
  PersistError(tempfile::PersistError)
}

impl fmt::Display for RatingsSaveError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::RatingsSaveError::*;
    match self {
      SerializeError(error) => write!(f, "Could not serialize the ratings map: {}", error),
      WriteError(error) => write!(f, "Could not write ratings to file: {}", error),
      PersistError(error) => write!(f, "Could not persist the temporary ratings file: {}", error),
    }
  }
}

impl Error for RatingsSaveError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::RatingsSaveError::*;
    match self {
      SerializeError(error) => Some(error),
      WriteError(error) => Some(error),
      PersistError(error) => Some(error)
    }
  }
}

impl From<serde_yaml::Error> for RatingsSaveError {
  fn from(error: serde_yaml::Error)->Self {
    RatingsSaveError::SerializeError(error)
  }
}

impl From<io::Error> for RatingsSaveError {
  fn from(error: io::Error)->Self {
    RatingsSaveError::WriteError(error)
  }
}

impl From<tempfile::PersistError> for RatingsSaveError {
  fn from(error: tempfile::PersistError)->Self {
    RatingsSaveError::PersistError(error)
  }
}


#[derive(Debug)]
pub enum RatingsLoadError {
  PathIsDir,
  FileOpenError(io::Error),
  DeserializeError(serde_yaml::Error),
}

impl fmt::Display for RatingsLoadError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::RatingsLoadError::*;
    match self {
      PathIsDir => write!(f, "The path to the image ratings file is a directory."),
      FileOpenError(error) => write!(f, "Could not open the ratings file: {}", error),
      DeserializeError(error) => write!(f, "Could not deseralize the contents of the ratings file: {}", error),
    }
  }
}

impl Error for RatingsLoadError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::RatingsLoadError::*;
    match self {
      PathIsDir => None,
      FileOpenError(error) => Some(error),
      DeserializeError(error) => Some(error)
    }
  }
}



impl From<io::Error> for RatingsLoadError {
  fn from(error: io::Error)->Self {
    RatingsLoadError::FileOpenError(error)
  }
}

impl From<serde_yaml::Error> for RatingsLoadError {
  fn from(error: serde_yaml::Error)->Self {
    RatingsLoadError::DeserializeError(error)
  }
}