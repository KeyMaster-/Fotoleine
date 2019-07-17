use std::error::Error;
use std::borrow::Cow;
use std::rc::Rc;
use std::path::Path;
use std::fs::{self, DirEntry};
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
  texture::{ClientFormat, RawImage2d},
  Texture2d
};
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use stb_image::image::{self, LoadResult, Image};

mod support;

struct ImguiImage {
  id: TextureId,
  size: [usize; 2]
}

struct PlacedImage {
  image: ImguiImage,
  pos: [f32; 2],
  scale: f32
}

struct Fotoleine {
  framework: Framework,
  view_area_size: [f32; 2],
  root_path: Option<Box<Path>>,
  image_entries: Option<Vec<DirEntry>>,
  image_idx: i32,
  image: Option<PlacedImage>
}

impl ImguiImage {
  fn from_data<F: Facade>(image:Image<u8>, gl_ctx:&F, textures: &mut Textures<Rc<Texture2d>>)->Result<ImguiImage, Box<dyn Error>> {
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

    let size = [width, height];
    
    let gl_texture = Texture2d::new(gl_ctx, raw_img)?;
    let id = textures.insert(Rc::new(gl_texture));

    Ok(ImguiImage {
      id,
      size
    })
  }
}

const AREA_FLAGS:ImGuiWindowFlags = ImGuiWindowFlags::from_bits_truncate(ImGuiWindowFlags::NoTitleBar.bits() | ImGuiWindowFlags::NoResize.bits() | ImGuiWindowFlags::NoMove.bits() | ImGuiWindowFlags::NoScrollbar.bits() | ImGuiWindowFlags::NoScrollWithMouse.bits() | ImGuiWindowFlags::NoCollapse.bits());

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

    self.framework.renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");

    loop_signal
  }
}

impl PlacedImage {
  fn scaled_size(&self)->[f32; 2] {
    let mut size = [self.image.size[0] as f32, self.image.size[1] as f32];
    size[0] *= self.scale;
    size[1] *= self.scale;
    return size;
  }

    // sets scale to fit into a rectangle of `size`, and centers itself within that rectangle
  fn place_to_fit(&mut self, size:[f32; 2], padding:f32) {
    let x_scale = size[0] / ((self.image.size[0] as f32) + padding);
    let y_scale = size[1] / ((self.image.size[1] as f32) + padding);
    self.scale = x_scale.min(y_scale);

    let scaled_size = self.scaled_size();
    self.pos[0] = size[0] / 2.0 - scaled_size[0] / 2.0;
    self.pos[1] = size[1] / 2.0 - scaled_size[1] / 2.0;
  }
}

impl Fotoleine {
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
  fn set_path(&mut self, path:Box<Path>)->bool { //, gl_ctx:&F, textures: &mut Textures<Rc<Texture2d>>
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
  fn load_image(&mut self, idx: usize) { // , gl_ctx:&F, textures: &mut Textures<Rc<Texture2d>>
    if self.image_entries.is_none() {
      return;
    }

    let path = self.image_entries.as_ref().unwrap()[idx].path();
    let img_res = image::load(path);

    if let LoadResult::ImageU8(img) = img_res {
      let gl_ctx = self.framework.display.get_context();
      let textures = self.framework.renderer.textures();
      let imgui_img_res = ImguiImage::from_data(img, gl_ctx, textures);
      self.image = imgui_img_res.ok().map(|imgui_img| { // ok() converts Ok(img) to Some(img), and Err(...) to None. 
        PlacedImage {
          image: imgui_img,
          pos: [0.0, 0.0],
          scale: 0.25
        }
      });
    }
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
  }

  fn build_ui(&mut self, ui:&mut Ui) {
      //:todo: this is silly, I'm doing work to undo most of what imgui is doing for me
      // better to do image drawing outside of imgui with glium directly, and draw imgui around that for what is needed
    {
      let _area_style_token = ui.push_style_vars(&[
        StyleVar::WindowPadding([0.0, 0.0]), 
        StyleVar::WindowRounding(0.0), 
        StyleVar::WindowBorderSize(0.0)]);

      ui.window(im_str!("ImgDisplay"))
        .position([0.0, 0.0], Condition::Always)
        .size(self.view_area_size, Condition::Always)
        .flags(AREA_FLAGS)
        .build(|| {
          if let Some(ref placed_img) = self.image {
            ui.set_cursor_pos(placed_img.pos);
            ui.image(placed_img.image.id, placed_img.scaled_size())
              .build();
          }
        });
    }
  }
}

fn main() {
  let display_size = [1280, 720];
  let (event_loop, imgui, framework) = init("fotoleine", display_size);
  let fotoleine = Fotoleine {
    framework: framework,
    view_area_size: [display_size[0] as f32, display_size[1] as f32],
    root_path: None,
    image_entries: None,
    image_idx: 0,
    image: None,
  };

  run(event_loop, imgui, fotoleine);
}
