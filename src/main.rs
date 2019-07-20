use std::error::Error;
use std::io;
use std::path::Path;
use std::fs::{self, DirEntry};
use std::process::Command;
use std::fmt;
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
  VertexBuffer,
  index::{NoIndices, PrimitiveType},
  implement_vertex, uniform
};
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use image::{ImageData, PlacedImage};


mod support;
mod image;

  // :todo: consider using snafu, io error has specific context of being during entry reading
  // issue is easy From trait implementations for use in ImageData::load
#[derive(Debug)]
enum DirLoadError {
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

  // A loaded directory of images we want to display
struct LoadedDir {
  path: Box<Path>,
  entries: Vec<DirEntry>,
  shown_idx: usize,
  shown_image: PlacedImage
}

impl LoadedDir {
  fn new<F: Facade>(path: &Path, gl_ctx: &F)->Result<LoadedDir, DirLoadError> {
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

    let shown_image = PlacedImage {
      image: image_data,
      pos: [0.0, 0.0],
      scale: 1.0
    };

    Ok(LoadedDir {
      path,
      entries,
      shown_idx,
      shown_image
    })
  }

  fn change_image<F: Facade>(&mut self, offset: i32, gl_ctx: &F)->Result<(), image::ImageLoadError> {
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
    self.shown_image = PlacedImage {
      image: image_data,
      pos: [0.0, 0.0],
      scale: 1.0
    };
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

#[derive(Copy, Clone, Debug)]
struct Vertex {
  pos: [f32; 2],
  tex_coord: [f32; 2],
}
implement_vertex!(Vertex, pos, tex_coord);

struct Fotoleine {
  framework: Framework,
  view_area_size: [f32; 2],
  loaded_dir: Option<LoadedDir>,

  img_draw_program: glium::Program,
  img_vert_buf: VertexBuffer<Vertex>,
  img_idx_buf: NoIndices,
  img_draw_matrix: [[f32; 4]; 4], 
}

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
            self.load_path(path);
            if let Some(ref mut loaded_dir) = self.loaded_dir {
              loaded_dir.shown_image.place_to_fit(self.view_area_size, 20.0);
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

    if let Some(ref mut loaded_dir) = self.loaded_dir {
      let gl_ctx = self.framework.display.get_context();
      let change_image_res = 
        if ui.is_key_pressed(VirtualKeyCode::A as _) {
          loaded_dir.change_image(-1, gl_ctx)
        } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
          loaded_dir.change_image( 1, gl_ctx)
        } else {
          Ok(())
        };

      match change_image_res {
        Ok(_) => {
          loaded_dir.shown_image.place_to_fit(self.view_area_size, 20.0); // :todo: unify with path load
        },
        Err(error) => {
          println!("Error changing image: {}", error);
        }
      }

      if ui.is_key_pressed(VirtualKeyCode::O as _) {
        let mut path = loaded_dir.entries[loaded_dir.shown_idx].path();
        path.set_extension("cr2");

        let open_res = Command::new("open")
          .arg(path.as_os_str())
          .output();

        if let Err(err) = open_res {
          println!("Couldn't open file {}, error {}", path.to_string_lossy(), err);
        }
      }
    }

    self.build_ui(&mut ui);

    let draw_data = end_frame(ui, &self.framework.platform, &self.framework.display);

    let mut target = self.framework.display.draw();
    target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);

    if let Some(ref loaded_dir) = self.loaded_dir {
      let mut corner_data = loaded_dir.shown_image.corner_data(); // ordered tl, tr, br, bl
      corner_data.swap(2, 3); // make the order tl, tr, br, bl, as needed for the triangle strip
      let verts: Vec<_> = corner_data.iter().map(|&(pos, tex_coord)| Vertex{pos, tex_coord}).collect();

      self.img_vert_buf.write(&verts);

      let uniforms = uniform! {
        transform: self.img_draw_matrix,
        img: &loaded_dir.shown_image.image.texture
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
      loaded_dir: None,

      img_draw_program: gl_program,
      img_vert_buf: vertex_buffer,
      img_idx_buf: index_buffer,
      img_draw_matrix: display_to_gl
    })
  }

  fn load_path(&mut self, path:&Path) {
    let gl_ctx = self.framework.display.get_context();
    self.loaded_dir = LoadedDir::new(&path, gl_ctx).ok();
  }

  fn build_ui(&mut self, ui:&mut Ui) {
  }
}

fn main() {
  let display_size = [1280, 720];
  let (event_loop, imgui, framework) = init("fotoleine", display_size);
  let fotoleine = Fotoleine::init(framework, &[display_size[0] as f32, display_size[1] as f32]).expect("Couldn't run Fotoleine init.");

  run(event_loop, imgui, fotoleine);
}
