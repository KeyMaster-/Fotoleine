use std::error::Error;
use std::borrow::Cow;
use std::path::Path;
use std::fs::{self, DirEntry};
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
  texture::{ClientFormat, RawImage2d, srgb_texture2d::SrgbTexture2d},
  VertexBuffer,
  index::{NoIndices, PrimitiveType},
  implement_vertex, uniform
};
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use stb_image::image::{self, LoadResult, Image};
use exif;

mod support;

  // Rotation that should be applied when displaying an image
  // to make it appear as it was taken.
enum ImageRotation { 
  None,
  NinetyCW,
  NinetyCCW,
  OneEighty
}

struct ImageData {
  texture: SrgbTexture2d,
  size: [usize; 2],
  rotation: ImageRotation
}

struct PlacedImage {
  image: ImageData,
  pos: [f32; 2],
  scale: f32
}

struct Fotoleine {
  framework: Framework,
  view_area_size: [f32; 2],
  root_path: Option<Box<Path>>,
  image_entries: Option<Vec<DirEntry>>,
  image_idx: i32,
  image: Option<PlacedImage>,

  img_draw_program: glium::Program,
  img_vert_buf: VertexBuffer<Vertex>,
  img_idx_buf: NoIndices,
  img_draw_matrix: [[f32; 4]; 4], 
}

impl ImageData {
  fn from_components<F: Facade>(image: Image<u8>, exif_orientation: Option<&exif::Field>, gl_ctx: &F)->Result<ImageData, Box<dyn Error>> {
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

  fn rotated_size(&self)->[usize; 2] {
    match self.rotation {
      ImageRotation::None | ImageRotation::OneEighty => [self.size[0], self.size[1]],
      ImageRotation::NinetyCW | ImageRotation::NinetyCCW => [self.size[1], self.size[0]]
    }
  }
}

// const AREA_FLAGS:ImGuiWindowFlags = ImGuiWindowFlags::from_bits_truncate(ImGuiWindowFlags::NoTitleBar.bits() | ImGuiWindowFlags::NoResize.bits() | ImGuiWindowFlags::NoMove.bits() | ImGuiWindowFlags::NoScrollbar.bits() | ImGuiWindowFlags::NoScrollWithMouse.bits() | ImGuiWindowFlags::NoCollapse.bits());

#[derive(Copy, Clone, Debug)]
struct Vertex {
  pos: [f32; 2],
  tex_coord: [f32; 2],
}
implement_vertex!(Vertex, pos, tex_coord);

impl Program for Fotoleine {
  fn framework(&self)->&Framework {
    return &self.framework;
  }

  fn framework_mut(&mut self)->&mut Framework {
    return &mut self.framework;
  }

  fn on_event(&mut self, event:&Event<()>)->LoopSignal {

    let loop_signal = match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::CloseRequested 
            => LoopSignal::Exit,
          WindowEvent::Resized { .. } | WindowEvent::Focused { .. } | WindowEvent::HiDpiFactorChanged { .. } |
          WindowEvent::KeyboardInput { .. } | 
          WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } |
          WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } 
            => LoopSignal::Redraw,
          _ => LoopSignal::Wait
        }
      },
      _ => LoopSignal::Wait
    };

    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::DroppedFile(path) => {
            self.set_path(path.clone().into_boxed_path());
            if let Some(ref mut placed_img) = self.image {
              placed_img.place_to_fit(self.view_area_size, 20.0);
            }
          }
          _ => {}
        }
      },
      _ => {}
    };

    loop_signal
  }

  fn on_frame(&mut self, imgui: &mut Context)->LoopSignal {
    let mut loop_signal = LoopSignal::Wait;

    let mut ui = begin_frame(imgui, &self.framework.platform, &self.framework.display);

    if ui.is_key_pressed(VirtualKeyCode::Q as _) && ui.io().key_super {
      loop_signal = LoopSignal::Exit;
    }

    if ui.is_key_pressed(VirtualKeyCode::A as _) {
      self.change_image(-1);
    } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
      self.change_image( 1);
    }

    self.build_ui(&mut ui);

    let draw_data = end_frame(ui, &self.framework.platform, &self.framework.display);

    let mut target = self.framework.display.draw();
    target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);

    if let Some(ref placed_img) = self.image {
      let mut corner_data = placed_img.corner_data(); // ordered tl, tr, br, bl
      corner_data.swap(2, 3); // make the order tl, tr, br, bl, as needed for the triangle strip
      let verts: Vec<_> = corner_data.iter().map(|&(pos, tex_coord)| Vertex{pos, tex_coord}).collect();

      self.img_vert_buf.write(&verts);

      let uniforms = uniform! {
        transform: self.img_draw_matrix,
        img: &placed_img.image.texture
      };

      target.draw(&self.img_vert_buf, &self.img_idx_buf, &self.img_draw_program, &uniforms, &Default::default()).expect("Drawing image geometry failed.");
    }

    self.framework.renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");

    loop_signal
  }
}

impl PlacedImage {
  fn scaled_size(&self)->[f32; 2] {
    let rotated_size = self.image.rotated_size();
    [(rotated_size[0] as f32) * self.scale, (rotated_size[1] as f32) * self.scale]
  }

  fn corner_data(&self)->[([f32; 2], [f32; 2]); 4] { // order: tl, tr, br, bl
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
  fn place_to_fit(&mut self, size:[f32; 2], padding:f32) {
    let rotated_size = self.image.rotated_size();

    let x_scale = size[0] / ((rotated_size[0] as f32) + padding);
    let y_scale = size[1] / ((rotated_size[1] as f32) + padding);
    self.scale = x_scale.min(y_scale);

    self.pos[0] = size[0] / 2.0;
    self.pos[1] = size[1] / 2.0;
  }
}

impl Fotoleine {
  fn init(framework: Framework, display_size:&[f32; 2])->Result<Fotoleine, Box<dyn Error>> {
    let vertex_buffer = VertexBuffer::empty_dynamic(&framework.display, 4)?;
    let index_buffer  = NoIndices(PrimitiveType::TriangleStrip);

    let vertex_shader_src = r#"
      #version 330

      uniform mat4 transform;

      in vec2 pos;
      in vec2 tex_coord;
      out vec2 f_tex_coord;

      void main() {
        f_tex_coord = tex_coord;
        gl_Position = transform * vec4(pos, 0.0, 1.0);
      }
    "#;

    let fragment_shader_src = r#"
      #version 330

      uniform sampler2D img;

      in vec2 f_tex_coord;
      out vec4 color;

      void main() {
        color = texture(img, f_tex_coord);
      }
    "#;

    let gl_program = glium::Program::from_source(&framework.display, vertex_shader_src, fragment_shader_src, None)?;

    let display_to_gl = 
      [[ 2.0 / display_size[0], 0.0, 0.0, 0.0],
       [ 0.0, -2.0 / display_size[1], 0.0, 0.0],
       [ 0.0,  0.0, 1.0, 0.0],
       [-1.0,  1.0, 0.0, 1.0f32]];

    Ok(Fotoleine {
      framework: framework,
      view_area_size: [display_size[0], display_size[1]],
      root_path: None,
      image_entries: None,
      image_idx: 0,
      image: None,

      img_draw_program: gl_program,
      img_vert_buf: vertex_buffer,
      img_idx_buf: index_buffer,
      img_draw_matrix: display_to_gl
    })
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

    // :todo: maybe make this return Result<(), Box<dyn Error>>
    // then all error handling can propagate outwards
  fn set_path(&mut self, path:Box<Path>)->bool {
    if path.is_dir() {
      self.root_path = Some(path);

      let dir_iter = fs::read_dir(self.root_path.as_ref().unwrap());
      if dir_iter.is_err() {
        return false;
      }

      self.image_entries = Some(dir_iter.unwrap()
        .filter_map(|entry_res| entry_res.ok())
        .filter(|entry| Fotoleine::is_relevant_file(entry))
        .collect());

      self.image_idx = 0;
      self.load_image(self.image_idx as usize);

      return true;
    } else {
      return false;
    }
  }

    //:todo: see if gl_ctx and Textures can be somehow moved into the struct so they don't have to pollute the arguments of everything that ever does image loading
  fn load_image(&mut self, idx: usize) {
    if self.image_entries.is_none() {
      return;
    }

    let path = self.image_entries.as_ref().unwrap()[idx].path();
    let img_res = image::load(&path);

    let exif_reader_opt = std::fs::File::open(&path).ok()
      .and_then(|file| exif::Reader::new(&mut std::io::BufReader::new(&file)).ok());

    if let (LoadResult::ImageU8(img), Some(exif_reader)) = (img_res, exif_reader_opt) {
      let gl_ctx = self.framework.display.get_context();
      let image_data_res = ImageData::from_components(img, exif_reader.get_field(exif::Tag::Orientation, false), gl_ctx);

      self.image = image_data_res.ok().map(|image_data| { // ok() converts Ok(img) to Some(img), and Err(...) to None. 
        PlacedImage {
          image: image_data,
          pos: [0.0, 0.0],
          scale: 1.0
        }
      });
    }

    // if let LoadResult::ImageU8(img) = img_res {
    //   let gl_ctx = self.framework.display.get_context();
    //   let image_data_res = ImageData::from_components(img, gl_ctx);
    //   self.image = image_data_res.ok().map(|image_data| { // ok() converts Ok(img) to Some(img), and Err(...) to None. 
    //     PlacedImage {
    //       image: image_data,
    //       pos: [0.0, 0.0],
    //       scale: 1.0
    //     }
    //   });
    // }

    // {
    //   let exif_reader_opt = std::fs::File::open(&path).ok()
    //     .and_then(|file| exif::Reader::new(&mut std::io::BufReader::new(&file)).ok());
    //     //.map(|exif_reader| {});
    //   if let Some(exif_reader) = exif_reader_opt {
    //     // let _fields = exif_reader.fields();
    //     exif_reader.get_field(exif::Tag::Orientation, false)
    //       .map(|field| {
    //         match field.value.get_uint(0) { // orientation is a vec of u16 values. Only one is expected, values 1 to 8, for different rotations and flips
    //           Some(1) => println!("No rotation, no flips"),
    //           Some(2) => println!("No rotation, x flip"),
    //           Some(3) => println!("180 cw or flip along x and y"),
    //           Some(4) => println!("No rotation, y flip"),
    //           Some(5) => println!("90 ccw, y flip"),
    //           Some(6) => println!("90 cw, no flips"),
    //           Some(7) => println!("90 cw, y flip"),
    //           Some(8) => println!("90 ccw"),
    //           _ => println!("Unknown orientatino value {:?}", field.value)
    //         }
    //       });
    //   }
    // }
  }

  fn change_image(&mut self, offset: i32) {
    if self.image_entries.is_none() {
      return;
    }

    let entries = self.image_entries.as_ref().unwrap();

    self.image_idx += offset;
    self.image_idx %= entries.len() as i32;
    if self.image_idx < 0 {
      self.image_idx += entries.len() as i32;
    }

    self.load_image(self.image_idx as usize);
    if let Some(ref mut placed_img) = self.image {
      placed_img.place_to_fit(self.view_area_size, 20.0);
    }
  }

  fn build_ui(&mut self, ui:&mut Ui) {
      //:todo: this is silly, I'm doing work to undo most of what imgui is doing for me
      // better to do image drawing outside of imgui with glium directly, and draw imgui around that for what is needed
    // {
    //   let _area_style_token = ui.push_style_vars(&[
    //     StyleVar::WindowPadding([0.0, 0.0]), 
    //     StyleVar::WindowRounding(0.0), 
    //     StyleVar::WindowBorderSize(0.0)]);

    //   ui.window(im_str!("ImgDisplay"))
    //     .position([0.0, 0.0], Condition::Always)
    //     .size(self.view_area_size, Condition::Always)
    //     .flags(AREA_FLAGS)
    //     .build(|| {
    //       if let Some(ref placed_img) = self.image {
    //         ui.set_cursor_pos(placed_img.pos);
    //         ui.image(placed_img.image.id, placed_img.scaled_size())
    //           .build();
    //       }
    //     });
    // }
  }
}

fn main() {
  let display_size = [1280, 720];
  let (event_loop, imgui, framework) = init("fotoleine", display_size);
  let fotoleine = Fotoleine::init(framework, &[display_size[0] as f32, display_size[1] as f32]).expect("Couldn't run Fotoleine init.");

  run(event_loop, imgui, fotoleine);
}
