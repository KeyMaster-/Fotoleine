use std::error::Error;
use std::process::Command;
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
};
use glium::glutin::event_loop::EventLoop;
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use glium::glutin::dpi::LogicalSize;
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use image_display::ImageDisplay;
use image_handling::{ImageHandling, loader_pool::LoadNotification};

mod support;
mod image;
mod image_handling;
mod image_display;
mod worker_pool;

struct Fotoleine {
  framework: Framework,
  image_handling: ImageHandling,
  image_display: ImageDisplay,
  view_area_size: LogicalSize,
}

impl Fotoleine {
  fn init(framework: Framework, display_size: &LogicalSize, event_loop: &EventLoop<LoadNotification>)->Result<Fotoleine, Box<dyn Error>> {
    let image_display = ImageDisplay::new(&framework.display, display_size)?;
    let image_handling = ImageHandling::new(10, 4, &event_loop);

    Ok(Fotoleine {
      framework,
      image_handling,
      image_display,
      view_area_size: display_size.clone(),
    })
  }

  fn build_ui(&mut self, ui:&mut Ui) {
    ui.window(im_str!("Hello world"))
        .size([300.0, 100.0], Condition::FirstUseEver)
        .build(|| {
            ui.text(im_str!("Hello world!"));
            ui.text(im_str!("こんにちは世界！"));
            ui.text(im_str!("This...is...imgui-rs!"));
            ui.separator();
            let mouse_pos = ui.io().mouse_pos;
            ui.text(format!(
                "Mouse Position: ({:.1},{:.1})",
                mouse_pos[0], mouse_pos[1]
            ));
        });
  }
}

impl Program for Fotoleine {
  type UserEvent = LoadNotification;

  fn framework(&self)->&Framework {
    return &self.framework;
  }

  fn framework_mut(&mut self)->&mut Framework {
    return &mut self.framework;
  }

  fn on_event(&mut self, event:&Event<Self::UserEvent>)->LoopSignal {
    let loop_signal = match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::CloseRequested 
            => LoopSignal::Exit,

            // On resize, need to use immediate redraw to update the visuals because a redraw request won't arrive until the resizing is done
            // Input events need to trigger an immediate redraw, since otherwise, both e.g. a key down and a key up event can arrive in the same batch
            //  In that case, a redraw request won't arrive until after both those events were processed, which means that imgui never sees the change from not pressed to pressed, effectively making it miss the input
            //  This should probably be done to all inputs, including cursor movement. However, redrawing on every cursor move makes the app feel more frame-y
          WindowEvent::Resized { .. } | 
          WindowEvent::KeyboardInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } 
            => LoopSignal::ImmediateRedraw,

          WindowEvent::Focused { .. } | WindowEvent::HiDpiFactorChanged { .. } |
          WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. }
            => LoopSignal::RequestRedraw,          

          _ => LoopSignal::Wait
        }
      },
      Event::UserEvent(_) => LoopSignal::RequestRedraw,
      _ => LoopSignal::Wait
    };

    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::DroppedFile(path) => {
            let load_res = self.image_handling.load_path(&path);
            if let Err(load_error) = load_res {
              println!("Couldn't load path {}: {}", path.to_string_lossy(), load_error);
            } else {
              if let Some(ref mut loaded_dir) = self.image_handling.loaded_dir {
                loaded_dir.set_shown(0, &self.image_handling.services);
              }
            }
          },
          WindowEvent::Resized(size) => {
            self.view_area_size = size.clone();
            self.image_display.set_display_size(size);
          },
          _ => {}
        }
      },
      Event::UserEvent(notification) => {
        println!("User event received, value {:?}", notification);
        match notification {
          LoadNotification::ImageLoaded => {
            if let Some(ref mut loaded_dir) = self.image_handling.loaded_dir {
              let gl_ctx = self.framework.display.get_context();
              let load_res = loaded_dir.receive_image(&self.image_handling.services, gl_ctx);
              if let Err(error) = load_res {
                println!("Error receiving image: {}", error);
              }
            } else {
                //:todo: this could happen if an invalid path was loaded while a load was pending
                // it's fine to discard the image in that case though
              println!("Received load result, but loaded_dir does not exist!");
            }
          },
          LoadNotification::LoadFailed => {
            println!("Image loading failed!");
            // :todo: set a flag and show this in the ui
            // also, maybe send image id along with notification to see whether the failed load was on the image we showed,
            // also to make decisions in loaded dir about re-requesting maybe
          }
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

    if let Some(ref mut loaded_dir) = self.image_handling.loaded_dir {
      if ui.is_key_pressed(VirtualKeyCode::A as _) {
        loaded_dir.set_shown(loaded_dir.offset_idx(-1), &self.image_handling.services);
      } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
        loaded_dir.set_shown(loaded_dir.offset_idx( 1), &self.image_handling.services);
      }

      if let Some(ref mut placed_image) = loaded_dir.image_at_mut(loaded_dir.shown_idx()) {
        placed_image.place_to_fit(&self.view_area_size, 20.0);
      };

      if ui.is_key_pressed(VirtualKeyCode::O as _) {
        let mut path = loaded_dir.path_at(loaded_dir.shown_idx());
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

    if let Some(ref loaded_dir) = self.image_handling.loaded_dir {
      if let Some(ref placed_image) = loaded_dir.image_at(loaded_dir.shown_idx()) {
        self.image_display.draw_image(placed_image, &mut target);
      }
    }

    self.framework.renderer
      .render(&mut target, draw_data)
      .expect("Rendering failed");
    target.finish().expect("Failed to swap buffers");

    loop_signal
  }

  fn on_shutdown(&mut self) {
    println!("Shutting down");
  }
}

fn main() {

  let display_size = LogicalSize::new(1280.0, 720.0);
  let (event_loop, imgui, framework) = init("fotoleine", &display_size);
  let fotoleine = Fotoleine::init(framework, &display_size, &event_loop).expect("Couldn't initialize Fotoleine.");

  run(event_loop, imgui, fotoleine);
}
