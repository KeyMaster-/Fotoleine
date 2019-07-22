use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use imgui::*;
use glium::{
  Surface,
  backend::Facade,
};
use glium::glutin::event_loop::{EventLoop, EventLoopProxy, EventLoopClosed};
use glium::glutin::event::{Event, WindowEvent, VirtualKeyCode};
use support::{init, Program, Framework, LoopSignal, run, begin_frame, end_frame};
use image::ImageData;
use loaded_dir::LoadedDir;
use image_display::ImageDisplay;
use worker_pool::{WorkerPool, Worker};
use std::sync::mpsc::Sender;

mod support;
mod image;
mod loaded_dir;
mod image_display;
mod worker_pool;

pub struct LoadWorker {
  id: usize,
  event_loop_proxy: EventLoopProxy<<Fotoleine as Program>::UserEvent>,
}

impl Worker for LoadWorker {
  type Input = (PathBuf, usize);
  type Output = (ImageData, usize);

  fn execute(&mut self, input: Self::Input, output: &Sender<Self::Output>) {
    let (path, idx) = input;
    let img_data_res = ImageData::load(&path);
    let event_message = 
      if let Ok(img_data) = img_data_res {
        let output_data = (img_data, idx);
        let send_res = output.send(output_data);
        match send_res {
          Ok(_) => {
            println!("Worker {}: channel send succeeded", self.id);
            LoadNotification::ImageLoaded
          },
          Err(error) => {
            println!("Worker {}: channel send failed, {}", self.id, error);
            LoadNotification::LoadFailed
          }
        }
      } else {
        LoadNotification::LoadFailed
      };

    match self.event_loop_proxy.send_event(event_message) {
      Ok(()) => println!("Worker {}: Send succeeded", self.id),
      Err(EventLoopClosed) => println!("Worker {}: Event loop closed", self.id)
    };
  }
}

type LoaderPool = WorkerPool<LoadWorker>;

struct Fotoleine {
  framework: Framework,
  view_area_size: [f32; 2],
  loaded_dir: Option<LoadedDir>,
  image_display: ImageDisplay,
  loader_pool: WorkerPool<LoadWorker>
}

  // using separate channels to notify about load, and actually send the load,
  // because the payload for winit user events is constrained to be Clone, which is not what I want.
#[derive(Debug)]
enum LoadNotification {
  ImageLoaded,
  LoadFailed
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
          WindowEvent::Resized { .. } | WindowEvent::Focused { .. } | WindowEvent::HiDpiFactorChanged { .. } |
          WindowEvent::KeyboardInput { .. } | 
          WindowEvent::CursorMoved { .. } | WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } |
          WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } 
            => LoopSignal::Redraw,
          _ => LoopSignal::Wait
        }
      },
      Event::UserEvent(_) => LoopSignal::Redraw,
      _ => LoopSignal::Wait
    };

    match event {
      Event::WindowEvent{event:win_event, .. } => {
        match win_event {
          WindowEvent::DroppedFile(path) => {
            self.loaded_dir = LoadedDir::new(&path).ok();
            if let Some(ref mut loaded_dir) = self.loaded_dir {
              loaded_dir.set_shown(0, &self.loader_pool);
            }
          }
          _ => {}
        }
      },
      Event::UserEvent(notification) => {
        println!("User event received, value {:?}", notification);
        match notification {
          LoadNotification::ImageLoaded => {
            let load_output_res = self.loader_pool.output.recv();
            if let Ok(load_output) = load_output_res {

              if let Some(ref mut loaded_dir) = self.loaded_dir {

                let loaded_idx = load_output.1;

                let gl_ctx = self.framework.display.get_context();
                let process_res = loaded_dir.process_loaded_image(load_output, gl_ctx);
                if let Err(error) = process_res {
                  println!("Couldn't process loaded image: {}", error);
                }

                if let Some(ref mut placed_image) = loaded_dir.image_at_mut(loaded_idx) {
                  placed_image.place_to_fit(self.view_area_size, 20.0);
                }

              } else {
                println!("Got load result despite loaded dir not existing?");
              }
            } else {
              println!("Error reaceiving loaded image data");
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

    if let Some(ref mut loaded_dir) = self.loaded_dir {
      if ui.is_key_pressed(VirtualKeyCode::A as _) {
        loaded_dir.set_shown(loaded_dir.offset_idx(-1), &self.loader_pool);
      } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
        loaded_dir.set_shown(loaded_dir.offset_idx( 1), &self.loader_pool);
      }

      if let Some(ref mut placed_image) = loaded_dir.image_at_mut(loaded_dir.shown_idx()) {
        placed_image.place_to_fit(self.view_area_size, 20.0);
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

    if let Some(ref loaded_dir) = self.loaded_dir {
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

impl Fotoleine {
  fn init(framework: Framework, display_size:&[f32; 2], event_loop: &EventLoop<<Self as Program>::UserEvent>)->Result<Fotoleine, Box<dyn Error>> {
    let image_display = ImageDisplay::new(&framework.display, &display_size)?;
    let loader_pool = WorkerPool::new(4, |id| {
      LoadWorker {
        id: id,
        event_loop_proxy: event_loop.create_proxy()
      }
    });


    Ok(Fotoleine {
      framework: framework,
      view_area_size: [display_size[0], display_size[1]],
      loaded_dir: None,
      image_display,
      loader_pool
    })
  }

  fn build_ui(&mut self, ui:&mut Ui) {
  }
}

fn main() {

  let display_size = [1280, 720];
  let (event_loop, imgui, framework) = init("fotoleine", display_size);
  let fotoleine = Fotoleine::init(framework, &[display_size[0] as f32, display_size[1] as f32], &event_loop).expect("Couldn't initialize Fotoleine.");

  run(event_loop, imgui, fotoleine);
}
