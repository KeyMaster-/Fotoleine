use std::path::Path;
use std::fs::{self, DirEntry};
use imgui::*;
use glium::Surface;
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};

mod support;

struct Fotoleine {
  root_path:Option<Box<Path>>
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
    let ext_matches = ext_str.unwrap().to_lowercase() == "jpg";

    let stem_str = path.file_stem().and_then(|stem| stem.to_str());
    if stem_str.is_none() { // no stem, or no unicode stem
      return false;
    }
    let stem_okay = !stem_str.unwrap().starts_with("._");

    ext_matches && stem_okay
  }

  fn set_path(&mut self, path:Box<Path>)->bool {
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
      return true;
    } else {
      return false;
    }
  }
}

impl Program for Fotoleine {
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

    build_ui(&mut ui);

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

fn build_ui(ui:&mut Ui) {
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
}

fn main() {
  let (event_loop, framework) = init("fotoleine");
  let fotoleine = Fotoleine {
    root_path: None
  };

  run(event_loop, framework, fotoleine);
}
