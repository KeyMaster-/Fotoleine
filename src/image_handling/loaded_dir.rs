use std::error::Error;
use std::io::{self, Write};
use std::fmt;
use std::path::{Path, PathBuf};
use std::fs::{self, File, DirEntry};
use std::collections::{HashMap, HashSet};
use glium::backend::Facade;
use glium::texture::TextureCreationError;
use crate::image::{ImageTexture, PlacedImage};
use super::ImageHandlingServices;

  // A loaded directory of images we want to display
pub struct LoadedDir {
  collection: Vec<DirEntry>,
  name_to_idx: HashMap<String, usize>,

  active_idxs: Vec<usize>, // List of image indices currently in the list that the user traverses. Indexes into collection
  load_pivot: usize, // indexes into active_idxs
  current_idx: usize, // current show image, indexes into active_idxs

  loaded_images: HashMap<usize, PlacedImage>, // all loaded images. keys index into collection
  pending_loads: HashSet<usize>, // keys index into collection

  ratings: ImageRatings,
  rating_filter: Option<Rating>
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

    let dir_iter = fs::read_dir(path)?;

    let mut collection: Vec<_> = dir_iter
      .filter_map(|entry_res| entry_res.ok())
      .filter(|entry| file_is_relevant(entry)) // filters for JPG files, and guarantees unicode filenames
      .collect();

    if collection.len() == 0 {
      return Err(DirLoadError::NoRelevantImages);
    }

    collection.sort_unstable_by_key(|entry| entry.file_name());

    let mut name_to_idx = HashMap::new();
    for (idx, entry) in collection.iter().enumerate() {
      let file_name = entry.file_name().into_string().unwrap();
      name_to_idx.insert(file_name, idx);
    }

    let active_idxs = (0..collection.len()).collect();
    let current_idx = 0;
    let load_pivot = 0;

    let loaded_images = HashMap::with_capacity(services.loading_policy.max_loaded_image_count());
    let pending_loads = HashSet::new();

    let ratings = ImageRatings::new(&path, &name_to_idx)?;

    let mut loaded_dir = LoadedDir {
      collection,
      name_to_idx,
      
      active_idxs,
      load_pivot,
      current_idx,

      loaded_images,
      pending_loads,
      ratings,
      rating_filter: None
    };

    loaded_dir.update_loaded(services);

    Ok(loaded_dir)
  }

  pub fn offset_current(&mut self, offset: i32, services: &ImageHandlingServices) {
    self.current_idx = offset_idx(self.current_idx, self.active_idxs.len(), offset);
    self.update_loaded(services);
  }

  pub fn current_collection_idx(&self)->usize {
    self.collection_idx(self.current_idx)
  }

  fn collection_idx(&self, idx: usize)->usize {
    self.active_idxs[idx]
  }

  pub fn collection_image_count(&self)->usize {
    self.collection.len()
  }

  pub fn current_image(&self)->Option<&PlacedImage> {
    self.loaded_images.get(&self.current_collection_idx())
  }

  pub fn current_image_mut(&mut self)->Option<&mut PlacedImage> {
    self.loaded_images.get_mut(&self.current_collection_idx())
  }

  pub fn current_path(&self)->PathBuf {
    self.collection[self.current_collection_idx()].path()
  }

  fn file_name_string(&self, coll_idx: usize)->String {
    self.collection[coll_idx].file_name().into_string().unwrap() // the image filter removes any entries which don't have a rust-string-representable filename
  }

  pub fn set_current_rating(&mut self, rating: Rating) {
    let file_name = self.file_name_string(self.current_collection_idx());
    let save_res = self.ratings.set_rating(file_name, rating);
    if let Err(error) = save_res {
      println!("Failed to save ratings: {}", error);
    }
  }

  pub fn get_current_rating(&self)->Rating {
    let file_name = self.file_name_string(self.current_collection_idx());
    self.ratings.get_rating(&file_name)
  }

  pub fn set_rating_filter(&mut self, rating: Option<Rating>, services: &ImageHandlingServices) {
    let new_active_idxs = 
      if let Some(rating) = rating {
        let file_names = self.ratings.filter_ratings(rating);
        let mut idxs: Vec<_> = file_names.iter().filter_map(|&file_name| self.name_to_idx.get(file_name)).map(|idx| *idx).collect();
        idxs.sort_unstable();

        idxs
      } else {
        (0..self.collection.len()).collect()
      };

    let coll_idx = self.current_collection_idx();
    let new_current = match new_active_idxs.binary_search(&coll_idx) {
      Ok(idx) => idx,
      Err(idx) => idx
    };
    let new_current = new_current.max(0).min(new_active_idxs.len() - 1);

    self.rating_filter = rating;
    self.active_idxs = new_active_idxs;
    self.load_pivot = new_current;
    self.current_idx = new_current;
    self.update_loaded(services);
  }

  pub fn get_rating_filter(&self)->Option<Rating> {
    self.rating_filter
  }

  fn update_loaded(&mut self, services: &ImageHandlingServices) {
    let (new_pivot, load_set) = services.loading_policy.get_load_set(self.load_pivot, self.current_idx, self.active_idxs.len());
    self.load_pivot = new_pivot;

    let load_coll_idxs: Vec<_> = load_set.iter().map(|&idx| self.collection_idx(idx)).collect();
    
    self.loaded_images.retain(|&key, _| {
      for &idx in &load_coll_idxs {
        if idx == key {
          return true;
        }
      }
      return false;
    });

    for coll_idx in load_coll_idxs {
      if self.needs_load(coll_idx) {
        self.submit_load_request(coll_idx, services);
      }
    }
  }

  fn needs_load(&self, coll_idx: usize)->bool {
    !self.loaded_images.contains_key(&coll_idx) && !self.pending_loads.contains(&coll_idx)
  }

  fn submit_load_request(&mut self, coll_idx: usize, services: &ImageHandlingServices) {
    let path = self.collection[coll_idx].path();
    self.pending_loads.insert(coll_idx);
    services.loader_pool.submit((path, coll_idx));
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
  NoRelevantImages,
  IoError(io::Error),
  RatingsLoadError(RatingsLoadError),
}

impl fmt::Display for DirLoadError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::DirLoadError::*;
    match self {
      NotADirectory => write!(f, "Given path is not a directory"),
      NoRelevantImages => write!(f, "Given directory does not contain any images to display"),
      IoError(error) => write!(f, "Could not read directory entries: {}", error),
      RatingsLoadError(error) => write!(f, "Could not load the ratings file: {}", error),
    }
  }
}

impl Error for DirLoadError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::DirLoadError::*;
    match self {
      NotADirectory => None,
      NoRelevantImages => None,
      IoError(error) => Some(error),
      RatingsLoadError(error) => Some(error),
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

struct ImageRatings {
  ratings_data: RatingsData,
  folder_path: PathBuf,
  ratings_file_path: PathBuf,
}

impl ImageRatings {
    // the HashMap would ideally be a HashSet, but there doesn't seem to be an easy way to pretend it is one
  fn new<V>(folder_path: &Path, known_images: &HashMap<String, V>)->Result<ImageRatings, RatingsLoadError> {
    let folder_path = folder_path.to_path_buf();

    let mut ratings_file_path = folder_path.clone();
    ratings_file_path.push("ratings.yaml");

    let ratings_data = RatingsData::load(&ratings_file_path, known_images)?;

    Ok(ImageRatings {
      ratings_data,
      folder_path,
      ratings_file_path,
    })
  }

  fn set_rating(&mut self, img_name: String, rating: Rating)->Result<(), RatingsSaveError> {
    self.ratings_data.ratings.insert(img_name, rating);
    self.save_ratings()
  }

  fn get_rating(&self, img_name: &String)->Rating {
    *self.ratings_data.ratings.get(img_name).unwrap()
  }

  fn save_ratings(&self)->Result<(), RatingsSaveError> {
    let s = serde_yaml::to_string(&self.ratings_data)?;

    let mut tmp_file = tempfile::NamedTempFile::new_in(&self.folder_path)?;
    tmp_file.as_file_mut().write(s.as_bytes())?;
    tmp_file.persist(&self.ratings_file_path)?;

    Ok(())
  }

  fn filter_ratings(&self, rating: Rating)->Vec<&String> {
    self.ratings_data.ratings.iter().filter(|kv| *kv.1 == rating).map(|kv| kv.0).collect()
  }
}

#[derive(Debug, Copy, Clone, PartialEq)]
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

struct RatingsData {
  ratings: HashMap<String, Rating>,
  orphaned_ratings: HashMap<String, Rating>
}

impl RatingsData {
  fn load<V>(file_path: &Path, known_images: &HashMap<String, V>)->Result<RatingsData, RatingsLoadError> {
    if file_path.is_dir() {
      return Err(RatingsLoadError::PathIsDir);
    }

    let mut data = RatingsData {
      ratings: HashMap::with_capacity(known_images.len()),
      orphaned_ratings: HashMap::new()
    };

      // give all images a default Low rating
    for img_name in known_images.keys() {
      data.ratings.insert(img_name.clone(), Rating::Low);
    }

    if !file_path.exists() {
      return Ok(data);
    }

    let file = File::open(file_path)?;
    let mut deser_map: HashMap<String, u8> = serde_yaml::from_reader(file)?;

      // split the saved ratings into ratings that match up with images in the folder,
      // and 'orphaned' ratings that are ignored, but will be written out to file again on saving
    for (img_name, rating_u8) in deser_map.drain() {
      let rating = Rating::from_u8(rating_u8);
      if known_images.contains_key(&img_name) {
        data.ratings.insert(img_name, rating);
      } else {
        data.orphaned_ratings.insert(img_name, rating);
      }
    }

    Ok(data)
  }
}

use serde::ser::{Serialize, Serializer, SerializeMap};
impl Serialize for RatingsData {
    // merges ratings and orphaned_ratings, and writes them out as a string: u8 map. Ratings are converted to u8. The written map is also sorted by key.
  fn serialize<S>(&self, serializer: S)->Result<S::Ok, S::Error>
    where S: Serializer
  {
    let mut entries: Vec<_> = self.ratings.iter().chain(self.orphaned_ratings.iter()).collect();
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