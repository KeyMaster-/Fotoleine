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

struct Fotoleine {
  root_path:Option<Box<Path>>,
  image:Option<TextureId>,
  image_size: [f32; 2]
}

impl Fotoleine {
  fn filter_entry(entry:&DirEntry)->bool {
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
  fn set_path<F: Facade>(&mut self, path:Box<Path>, gl_ctx:&F, textures: &mut Textures<Rc<Texture2d>>)->bool {
    if path.is_dir() {
      self.root_path = Some(path);

      let dir_iter = fs::read_dir(self.root_path.as_ref().unwrap());
      if dir_iter.is_err() {
        return false;
      }

      let jpgs:Vec<_> = dir_iter.unwrap()
        .filter_map(|entry_res| entry_res.ok())
        .filter(|entry| Fotoleine::filter_entry(entry))
        .collect();

      println!("{:?}", jpgs);

      let img_res = image::load(jpgs[0].path());

      if let LoadResult::ImageU8(img) = img_res {
        let Image {
          width,
          height,
          data,
          ..
        } = img;

        let raw_img = RawImage2d {
          data: Cow::Owned(data),
          width: width as u32,
          height: height as u32,
          format: ClientFormat::U8U8U8,
        };
        
        let gl_texture_res = Texture2d::new(gl_ctx, raw_img);
        if let Ok(gl_texture) = gl_texture_res {
          let tex_id = textures.insert(Rc::new(gl_texture));
          self.image = Some(tex_id);
          self.image_size = [width as f32, height as f32];
        }
      }

      return true;
    } else {
      return false;
    }
  }
}

impl Program for Fotoleine {
  fn on_event(&mut self, event:&Event<()>, framework:&mut Framework)->LoopSignal {

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
            self.set_path(path.clone().into_boxed_path(), framework.display.get_context(), framework.renderer.textures());
          }
          _ => {}
        }
      },
      _ => {}
    };

    loop_signal
  }

  fn on_frame(&mut self, framework:&mut Framework)->LoopSignal {
    let Framework {
      ref display,
      ref platform,
      ref mut imgui,
      ref mut renderer
    } = framework;

    let mut loop_signal = LoopSignal::Wait;

    let mut ui = begin_frame(imgui, platform, display);

    self.build_ui(&mut ui);

    if ui.is_key_pressed(VirtualKeyCode::Q as _) && ui.io().key_super {
      loop_signal = LoopSignal::Exit;
    }

    let draw_data = end_frame(ui, platform, display);

    let mut target = display.draw();
    target.clear_color_srgb(0.1, 0.1, 0.1, 1.0);

    renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");

    loop_signal
  }
}

impl Fotoleine {
  fn build_ui(&mut self, ui:&mut Ui) {
    ui.window(im_str!("Hello world"))
      .size([300.0, 100.0], Condition::FirstUseEver)
      .build(|| {
        ui.text(im_str!("Hello world!"));
        ui.text(im_str!("こんにちは世界！"));
        ui.text(im_str!("This...is...imgui-rs!"));
        ui.separator();
        // ui.input_text()
        let mouse_pos = ui.io().mouse_pos;
        ui.text(format!(
          "Mouse Position: ({:.1},{:.1})",
          mouse_pos[0], mouse_pos[1]
        ));
      });

    if let Some(tex_id) = self.image {
      ui.image(tex_id, self.image_size)
        .size([648.0, 432.0])
        .build();
    }
  }
}



fn main() {
  let (event_loop, framework) = init("fotoleine");
  let fotoleine = Fotoleine {
    root_path: None,
    image: None,
    image_size: [0.0, 0.0]
  };

  run(event_loop, framework, fotoleine);
}
