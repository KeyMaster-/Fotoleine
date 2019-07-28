use std::error::Error;
use std::io;
use std::path::Path;
use glium::{
  backend::Facade,
  texture::{RawImage2d, CompressedSrgbTexture2d, TextureCreationError},
};
use glium::glutin::dpi::{LogicalSize, LogicalPosition};
use stb_image::image::{Image, LoadResult};
use exif;

  // Rotation that should be applied when displaying an image
  // to make it appear as it was taken.
pub enum ImageRotation { 
  None,
  NinetyCW,
  NinetyCCW,
  OneEighty
}

pub struct ImageData {
  image: Image<u8>,
  rotation: ImageRotation
}

impl ImageData {
  pub fn load(path: &Path)->Result<ImageData, ImageLoadError> {
    let img_res = stb_image::image::load(&path);
    let image = match img_res {
      LoadResult::ImageU8(img) => img,
      LoadResult::Error(msg) => return Err(ImageLoadError::StbImageError(msg)),
      LoadResult::ImageF32(_) => return Err(ImageLoadError::FloatImage),
    };

    let img_file = std::fs::File::open(&path)?;
    let exif_reader = exif::Reader::new(&mut std::io::BufReader::new(&img_file))?;
    let orientation_field = exif_reader.get_field(exif::Tag::Orientation, false);

    let rotation = orientation_field.map_or(ImageRotation::None, |orientation_field| {
      match orientation_field.value.get_uint(0) { // orientation is a vec of u16 values. Only one is expected, values 1 to 8, for different rotations and flips
        Some(1) => ImageRotation::None,
        Some(3) => ImageRotation::OneEighty,
        Some(6) => ImageRotation::NinetyCW,
        Some(8) => ImageRotation::NinetyCCW,
        Some(id) => {
          println!("Orientation {} is not supported.", id); // 2, 4, 5, 7
          ImageRotation::None
        },
        None => {
          println!("Unknown orientation value {:?}", orientation_field);
          ImageRotation::None
        }
      }
    });

    Ok(ImageData {
      image,
      rotation
    })
  }
}

pub struct ImageTexture {
  pub texture: CompressedSrgbTexture2d,
  pub size: [usize; 2],
  pub rotation: ImageRotation
}

impl ImageTexture {
  pub fn from_data<F: Facade>(data: ImageData, gl_ctx: &F)->Result<ImageTexture, TextureCreationError> {
    let ImageData {
      image, 
      rotation
    } = data;

    let Image {
      width,
      height,
      data,
      ..
    } = image;

    let raw_img = RawImage2d::from_raw_rgb(data, (width as u32, height as u32));
    let texture = CompressedSrgbTexture2d::new(gl_ctx, raw_img)?;
    let size = [width, height];

    Ok(ImageTexture {
      texture,
      rotation,
      size
    })
  }

  pub fn rotated_size(&self)->[usize; 2] {
    match self.rotation {
      ImageRotation::None | ImageRotation::OneEighty => [self.size[0], self.size[1]],
      ImageRotation::NinetyCW | ImageRotation::NinetyCCW => [self.size[1], self.size[0]]
    }
  }
}

pub struct PlacedImage {
  pub image: ImageTexture,
  pub pos: LogicalPosition,
  pub scale: f64
}

impl PlacedImage {
  pub fn new(image: ImageTexture)->PlacedImage {
    PlacedImage {
      image: image,
      pos: LogicalPosition::new(0.0, 0.0),
      scale: 1.0
    }
  }

  pub fn scaled_size(&self)->LogicalSize {
    let rotated_size = self.image.rotated_size();

    LogicalSize::new((rotated_size[0] as f64) * self.scale, (rotated_size[1] as f64) * self.scale)
  }

  pub fn corner_data(&self)->[(LogicalPosition, [f32; 2]); 4] { // order: tl, tr, br, bl
    let scaled_size = self.scaled_size();

    let pos = [LogicalPosition::new(self.pos.x - scaled_size.width / 2.0, self.pos.y - scaled_size.height / 2.0),
               LogicalPosition::new(self.pos.x + scaled_size.width / 2.0, self.pos.y - scaled_size.height / 2.0),
               LogicalPosition::new(self.pos.x + scaled_size.width / 2.0, self.pos.y + scaled_size.height / 2.0),
               LogicalPosition::new(self.pos.x - scaled_size.width / 2.0, self.pos.y + scaled_size.height / 2.0)];

    let rotation_steps = match self.image.rotation {
      ImageRotation::None => 0,
      ImageRotation::NinetyCW => 1,
      ImageRotation::OneEighty => 2,
      ImageRotation::NinetyCCW => 3
    };

    let mut uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    uv.rotate_right(rotation_steps);

    [(pos[0], uv[0]), (pos[1], uv[1]), (pos[2], uv[2]), (pos[3], uv[3])]
  }

    // sets scale to fit into a rectangle of `size`, and centers itself within that rectangle
  pub fn place_to_fit(&mut self, size: &LogicalSize, padding: f64) {
    let rotated_size = self.image.rotated_size();

    let x_scale = size.width / ((rotated_size[0] as f64) + padding);
    let y_scale = size.height / ((rotated_size[1] as f64) + padding);
    self.scale = x_scale.min(y_scale);

    self.pos.x = size.width / 2.0;
    self.pos.y = size.height / 2.0;
  }
}

#[derive(Debug)]
pub enum ImageLoadError {
  FloatImage,
  StbImageError(String),
  IoError(io::Error),
  ExifError(exif::Error)
}

use std::fmt;
impl fmt::Display for ImageLoadError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::ImageLoadError::*;
    match self {
      FloatImage => write!(f, "stb_image returned an F32 image, which is not handled currently."),
      StbImageError(error) => write!(f, "stb_image load error: {}", error),
      IoError(error) => write!(f, "File read error: {}", error),
      ExifError(error) => write!(f, "Could not read exif data: {}", error),
    }
  }
}

impl Error for ImageLoadError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::ImageLoadError::*;
    match self {
      IoError(error) => Some(error),
      ExifError(error) => Some(error),
      _ => None
    }
  }
}

impl From<io::Error> for ImageLoadError {
  fn from(error: io::Error)->Self {
    ImageLoadError::IoError(error)
  }
}

impl From<exif::Error> for ImageLoadError {
  fn from(error: exif::Error)->Self {
    ImageLoadError::ExifError(error)
  }
}