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
use image_handling::{ImageHandling, loader_pool::LoadNotification, Rating};

mod support;
mod image;
mod image_handling;
mod image_display;
mod worker_pool;

const INVIS_WINDOW_FLAGS: ImGuiWindowFlags = ImGuiWindowFlags::from_bits_truncate(ImGuiWindowFlags::NoBackground.bits() | ImGuiWindowFlags::NoDecoration.bits() | ImGuiWindowFlags::NoInputs.bits() | ImGuiWindowFlags::NoSavedSettings.bits());

struct Fotoleine {
  framework: Framework,
  font: FontId,
  image_handling: ImageHandling,
  image_display: ImageDisplay,
  view_area_size: LogicalSize,
  bg_col: [f32; 3],
  show_ui: bool
}

impl Fotoleine {
  fn init(mut framework: Framework, display_size: &LogicalSize, imgui: &mut Context, event_loop: &EventLoop<LoadNotification>)->Result<Fotoleine, FotoleineInitError> {
    let image_display = ImageDisplay::new(&framework.display, display_size)?;
      // 2 images on either side of shown that can be flicked between without triggering loads. 
      // keep 2 images before the buffer zone
      // load the next 5 images after the buffer zone
      //   For a total of 1 + 2 * 2 + 2 + 5 = 12 loaded images at any time
      // have 4 worker threads
    let image_handling = ImageHandling::new(2, 2, 5, 4, &event_loop);

      // consider moving this and the font id storage into framework
    let inter_font = imgui.fonts().add_font(&[
      FontSource::TtfData {
        data: include_bytes!("../resources/Inter-Light-BETA.ttf"),
        size_pixels: (18.0 * framework.platform.hidpi_factor()) as f32,
        config: None,
      }
    ]);

    framework.renderer.reload_font_texture(imgui)
      .expect("Couldn't reload font");

    Ok(Fotoleine {
      framework,
      font: inter_font,
      image_handling,
      image_display,
      view_area_size: display_size.clone(),
      bg_col: [0.1, 0.1, 0.1],
      show_ui: true
    })
  }

  fn build_ui(&mut self, ui:&mut Ui) {
    let _font = ui.push_font(self.font);
      // disable anything messing with the window drawing area, such that the UI window actually covers the entire drawing area
    let _window_border = ui.push_style_vars(&[StyleVar::WindowBorderSize(0.0), StyleVar::WindowRounding(0.0), StyleVar::WindowPadding([0.0, 0.0])]);
    ui.window(im_str!("overlay"))
      .flags(INVIS_WINDOW_FLAGS)
      .position([0.0, 0.0], Condition::Always)
      .size([self.view_area_size.width as f32, self.view_area_size.height as f32], Condition::Always) // :todo: currently assumes view area size is full screen size
      .build(|| {
        if let Some(ref loaded_dir) = self.image_handling.loaded_dir {
          if self.show_ui {
            let border_padding = 10.0; // distance between the window edge and the border of the backing box
            let backing_padding_x = 10.0; // distance between the backing box edge and actual content, left and right edge
            let backing_padding_y = 15.0; // same as above, but top/bottom edge
            let backing_col = [self.bg_col[0], self.bg_col[1], self.bg_col[2], 0.5];
            let text_top_adjust = 5.0; // for layout, the top of the text bounding box is moved down by this much.
            let text_height_adjust = 5.0; // the amount of space to remove from the bottom of the text height, to get better spacing and alignment overall. Necessary since I can't get the text baseline position from imgui

            let rating_line_spacing = 20.0;
            let filter_border_padding = 5.0;

              // image index in folder
            let collection_count = loaded_dir.collection_image_count();
            let collection_idx = loaded_dir.current_collection_idx() + 1;
            let count_str = format!("{}", collection_count);

            let text = ImString::new(format!("{}/{}", collection_idx, count_str));
            let mut text_size = ui.calc_text_size(&text, false, -1.0); // -1.0 means no wrap width
            text_size[1] -= text_height_adjust + text_top_adjust;

            let widest_text = ImString::new(format!("{}/{}", count_str, count_str));
            let widest_size = ui.calc_text_size(&widest_text, false, -1.0);

              // dimensions of UI drawing area
            let ui_box_right = self.view_area_size.width as f32 - border_padding - backing_padding_x;
            let ui_box_left = ui_box_right - widest_size[0];
            let ui_box_bot = self.view_area_size.height as f32 - border_padding - backing_padding_y;
            let ui_box_top = ui_box_bot - text_size[1] - backing_padding_y - rating_line_spacing * (Rating::max() as f32);

            {
              let draw_list = ui.get_window_draw_list();

              let backing_tl = [ui_box_left - backing_padding_x, ui_box_top - backing_padding_y];
              let backing_br = [ui_box_right + backing_padding_x, ui_box_bot + backing_padding_y];
              draw_list.add_rect(backing_tl, backing_br, backing_col).filled(true).build();

              let text_left = ui_box_left + (ui_box_right - ui_box_left) / 2.0 - text_size[0] / 2.0;
              let text_top = ui_box_bot - text_size[1];
              draw_list.add_text([text_left, text_top - text_top_adjust], [1.0, 1.0, 1.0, 1.0], text); // move up by the adjustment amount since the actual visual text is drawn that much further down from the top-left position given to imgui

              let rating_num = loaded_dir.get_current_rating().to_u8();
              let line_left = ui_box_left;
              let line_right = ui_box_right;
              let line_base_height = text_top - backing_padding_y;
              for i in 0..=Rating::max() {
                let line_height = line_base_height - i as f32 * rating_line_spacing;
                let col = if rating_num == i {
                  [1.0, 1.0, 1.0, 1.0]
                } else {
                  [0.8, 0.8, 0.8, 1.0]
                };

                let dashed = rating_num != i;
                let target_dash_width = 5.0;
                let dash_gap_ratio = 0.3; // the gap width is the dash width * this ratio

                let target_stride_width = target_dash_width + target_dash_width * dash_gap_ratio;

                // the equation we're solving here is:
                // lw = n * w + (n - 1) * w * r
                //   where lw is the line width, n is the number of dashes, w is the dash width, and r is the dash gap ratio
                //   this expresses that the whole line width is covered by n dashes, with gaps after each dash, except for the last dash (we want the last dash to end at the right end of the line)

                // solve for n to get the "exact", decimal number of dashes required to cover lw:
                // lw = n * w + n * w * r - w * r
                // lw + w * r = n * (w + w * r)
                // n = (lw + w * r) / (w + w * r)

                // then we round that number to get to the closest whole number of dashes. 
                // we'll use that to then solve back to the actual dash width that covers the line width with a whole number of dashes
                
                let line_width = line_right - line_left;
                let n_dashes = ((line_width + target_dash_width * dash_gap_ratio) / target_stride_width).round();

                // to get the dash width, take the original equation, and solve for w (since now we know n)
                // lw = n * w + (n - 1) * w * r
                // lw = w * (n + (n - 1) * r)
                // w = lw / (n + (n - 1) * r)
                let dash_width = line_width / (n_dashes + (n_dashes - 1.0) * dash_gap_ratio);
                  // adjust the gap width to make sure it's an integer pixel amount, to have more consistent gap width when drawing.
                let gap_width = (dash_width * dash_gap_ratio).ceil();
                let dash_width = (dash_width + dash_width * dash_gap_ratio) - gap_width;
                let stride_width = dash_width + gap_width;

                if dashed {
                  for i in 0..(n_dashes as i32) {
                    let dash_start = line_left + (i as f32) * stride_width;
                    let dash_end = dash_start + dash_width;

                    draw_list.add_line([dash_start, line_height], [dash_end, line_height], col).build();
                  }
                } else {
                  draw_list.add_line([line_left, line_height], [line_right, line_height], col).build();
                }

                if let Some(filter_rating) = loaded_dir.get_rating_filter() {
                  if filter_rating.to_u8() == i {
                    draw_list.add_rect([line_left - filter_border_padding, line_height - filter_border_padding], [line_right + filter_border_padding + 1.0, line_height + filter_border_padding + 1.0], col).filled(false).build();
                  }
                }
              }
            }
          }

          {
            if let None = loaded_dir.current_image() {
              let text = im_str!("Image loading...");
              let text_size = ui.calc_text_size(&text, false, -1.0); // :todo: move out text alignment utilities into a function & module
              ui.set_cursor_pos([(self.view_area_size.width as f32) / 2.0 - text_size[0] / 2.0, (self.view_area_size.height as f32) / 2.0 - text_size[1] / 2.0]);
              ui.text(text);
            }
          }
        } else {
          let text = im_str!("Drag a folder with images into the window to load it.");
          let text_size = ui.calc_text_size(&text, false, -1.0);
          ui.set_cursor_pos([(self.view_area_size.width as f32) / 2.0 - text_size[0] / 2.0, (self.view_area_size.height as f32) / 2.0 - text_size[1] / 2.0]);
          ui.text(text);
        }
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

            // cursor moved not doing an instant redraw might mean that intermediate mouse positions are not detected on long blocking frames
            // so certain hover states may not be detected. this is deemed acceptable though, since doing immediate redraws on mouse movement has a noticeable impact on UI smootheness
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
              println!("Couldn't load path {}: {}", path.display(), load_error);
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
        loaded_dir.offset_current(-1, &self.image_handling.services);
      } else if ui.is_key_pressed(VirtualKeyCode::D as _) {
        loaded_dir.offset_current( 1, &self.image_handling.services);
      }

      if let Some(ref mut placed_image) = loaded_dir.current_image_mut() {
        placed_image.place_to_fit(&self.view_area_size, 0.0);
      };

      if ui.is_key_pressed(VirtualKeyCode::O as _) {
        let mut path = loaded_dir.current_path();
        path.set_extension("cr2");

        let open_res = Command::new("open")
          .arg(path.as_os_str())
          .output();

        if let Err(err) = open_res {
          println!("Couldn't open file {}, error {}", path.display(), err);
        }
      }

      if ui.is_key_pressed(VirtualKeyCode::U as _) {
        self.show_ui = !self.show_ui;
      }

      if ui.is_key_pressed(VirtualKeyCode::Escape as _) {
        loaded_dir.set_rating_filter(None, &self.image_handling.services);
      }

      if ui.io().key_super {
        if ui.is_key_pressed(VirtualKeyCode::Key1 as _) {
          loaded_dir.set_rating_filter(Some(Rating::Low), &self.image_handling.services);
        } else if ui.is_key_pressed(VirtualKeyCode::Key2 as _) {
          loaded_dir.set_rating_filter(Some(Rating::Medium), &self.image_handling.services);
        } else if ui.is_key_pressed(VirtualKeyCode::Key3 as _) {
          loaded_dir.set_rating_filter(Some(Rating::High), &self.image_handling.services);
        }
      } else {
        if ui.is_key_pressed(VirtualKeyCode::Key1 as _) {
          loaded_dir.set_current_rating(Rating::Low);
        } else if ui.is_key_pressed(VirtualKeyCode::Key2 as _) {
          loaded_dir.set_current_rating(Rating::Medium);
        } else if ui.is_key_pressed(VirtualKeyCode::Key3 as _) {
          loaded_dir.set_current_rating(Rating::High);
        }  
      }
    }

    self.build_ui(&mut ui);

    let draw_data = end_frame(ui, &self.framework.platform, &self.framework.display);

    let mut target = self.framework.display.draw();
    target.clear_color_srgb(self.bg_col[0], self.bg_col[1], self.bg_col[2], 1.0);

    if let Some(ref loaded_dir) = self.image_handling.loaded_dir {
      if let Some(ref placed_image) = loaded_dir.current_image() {
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
  }
}

fn main() {
  let display_size = LogicalSize::new(1280.0, 720.0);
  let (event_loop, mut imgui, framework) = init("Fotoleine", &display_size);
  let fotoleine = Fotoleine::init(framework, &display_size, &mut imgui, &event_loop).expect("Couldn't initialize Fotoleine.");

  run(event_loop, imgui, fotoleine);
}

#[derive(Debug)]
enum FotoleineInitError {
  ImageDisplayCreationError(image_display::ImageDisplayCreationError),
  GliumRendererError(imgui_glium_renderer::GliumRendererError),
}

use std::fmt;
impl fmt::Display for FotoleineInitError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>)->fmt::Result {
    use self::FotoleineInitError::*;
    match self {
      ImageDisplayCreationError(error) => write!(f, "Couldn't create the image display: {}", error),
      GliumRendererError(error) => write!(f, "Couldn't reload the font texture: {}", error),
    }
  }
}

impl Error for FotoleineInitError {
  fn source(&self)->Option<&(dyn Error + 'static)> {
    use self::FotoleineInitError::*;
    match self {
      ImageDisplayCreationError(error) => Some(error),
      GliumRendererError(_) => None, // glium renderer error doesn't impl Error, :todo:
    }
  }
}

impl From<image_display::ImageDisplayCreationError> for FotoleineInitError {
  fn from(error: image_display::ImageDisplayCreationError)->Self {
    FotoleineInitError::ImageDisplayCreationError(error)
  }
}

impl From<imgui_glium_renderer::GliumRendererError> for FotoleineInitError {
  fn from(error: imgui_glium_renderer::GliumRendererError)->Self {
    FotoleineInitError::GliumRendererError(error)
  }
}
