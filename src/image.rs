use std::error::Error;
use std::fmt;
use std::io;
use std::borrow::Cow;
use std::path::Path;
use glium::{
  backend::Facade,
  texture::{self, ClientFormat, RawImage2d, srgb_texture2d::SrgbTexture2d, TextureCreationError}
};
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
  pub texture: SrgbTexture2d,
  pub size: [usize; 2],
  pub rotation: ImageRotation
}

#[derive(Debug)]
pub enum ImageLoadError {
  FloatImage,
  StbImageError(String),
  IoError(io::Error),
  TextureCreationError(texture::TextureCreationError),
  ExifError(exif::Error)
}

impl fmt::Display for ImageLoadError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::ImageLoadError::*;
    match self {
      FloatImage => write!(f, "stb_image returned an F32 image, which is not handled currently."),
      StbImageError(error) => write!(f, "stb_image load error: {}", error),
      IoError(error) => write!(f, "File read error: {}", error),
      TextureCreationError(error) => write!(f, "Could not create texture: {}", error),
      ExifError(error) => write!(f, "Could not read exif data: {}", error),
    }
  }
}

impl Error for ImageLoadError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::ImageLoadError::*;
    match self {
      IoError(error) => Some(error),
      TextureCreationError(error) => Some(error),
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

impl From<texture::TextureCreationError> for ImageLoadError {
  fn from(error: texture::TextureCreationError)->Self {
    ImageLoadError::TextureCreationError(error)
  }
}

impl From<exif::Error> for ImageLoadError {
  fn from(error: exif::Error)->Self {
    ImageLoadError::ExifError(error)
  }
}

impl ImageData {
  pub fn load<F: Facade>(path: &Path, gl_ctx: &F)->Result<ImageData, ImageLoadError> {
    let img_res = stb_image::image::load(&path);
    let img = match img_res {
      LoadResult::Error(msg) => return Err(ImageLoadError::StbImageError(msg)),
      LoadResult::ImageU8(img) => img,
      LoadResult::ImageF32(_) => return Err(ImageLoadError::FloatImage)
    };

    let img_file = std::fs::File::open(&path)?;
    let exif_reader = exif::Reader::new(&mut std::io::BufReader::new(&img_file))?;
    let orientation_field = exif_reader.get_field(exif::Tag::Orientation, false);

    let image_data = ImageData::from_components(img, orientation_field, gl_ctx)?;
    Ok(image_data)
  }

  pub fn from_components<F: Facade>(image: Image<u8>, exif_orientation: Option<&exif::Field>, gl_ctx: &F)->Result<ImageData, TextureCreationError> {
    let Image {
      width,
      height,
      data,
      ..
    } = image;

    let raw_img = RawImage2d {
      data: Cow::Owned(data),
      width: width as u32,
      height: height as u32,
      format: ClientFormat::U8U8U8,
    };

    let texture = SrgbTexture2d::new(gl_ctx, raw_img)?;

    let size = [width, height];

    let rotation = exif_orientation.map_or(ImageRotation::None, |orientation_field| {
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
          println!("Unknown orientation value {:?}", exif_orientation);
          ImageRotation::None
        }
      }
    });

    Ok(ImageData {
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
  pub image: ImageData,
  pub pos: [f32; 2],
  pub scale: f32
}

impl PlacedImage {
  pub fn scaled_size(&self)->[f32; 2] {
    let rotated_size = self.image.rotated_size();
    [(rotated_size[0] as f32) * self.scale, (rotated_size[1] as f32) * self.scale]
  }

  pub fn corner_data(&self)->[([f32; 2], [f32; 2]); 4] { // order: tl, tr, br, bl
    let scaled_size = self.scaled_size();

    let pos = [[self.pos[0] - scaled_size[0] / 2.0, self.pos[1] - scaled_size[1] / 2.0],
               [self.pos[0] + scaled_size[0] / 2.0, self.pos[1] - scaled_size[1] / 2.0],
               [self.pos[0] + scaled_size[0] / 2.0, self.pos[1] + scaled_size[1] / 2.0],
               [self.pos[0] - scaled_size[0] / 2.0, self.pos[1] + scaled_size[1] / 2.0]];

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
  pub fn place_to_fit(&mut self, size:[f32; 2], padding:f32) {
    let rotated_size = self.image.rotated_size();

    let x_scale = size[0] / ((rotated_size[0] as f32) + padding);
    let y_scale = size[1] / ((rotated_size[1] as f32) + padding);
    self.scale = x_scale.min(y_scale);

    self.pos[0] = size[0] / 2.0;
    self.pos[1] = size[1] / 2.0;
  }
}