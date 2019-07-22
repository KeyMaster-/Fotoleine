use std::error::Error;
use glium::{
  Display, Frame, Surface,
  VertexBuffer,
  index::{NoIndices, PrimitiveType},
  implement_vertex, uniform
};
use crate::image::PlacedImage;

#[derive(Copy, Clone, Debug)]
struct Vertex {
  pos: [f32; 2],
  tex_coord: [f32; 2],
}
implement_vertex!(Vertex, pos, tex_coord);

pub struct ImageDisplay {
  program: glium::Program,
  vert_buf: VertexBuffer<Vertex>,
  idx_buf: NoIndices,
  view_matrix: [[f32; 4]; 4], 
}

impl ImageDisplay {
  pub fn new(display: &Display, display_size:&[f32; 2])->Result<ImageDisplay, Box<dyn Error>> { //:todo: custom error
    let vertex_buffer = VertexBuffer::empty_dynamic(display, 4)?;
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

    let program = glium::Program::from_source(display, vertex_shader_src, fragment_shader_src, None)?;

    let display_to_gl = 
      [[ 2.0 / display_size[0], 0.0, 0.0, 0.0],
       [ 0.0, -2.0 / display_size[1], 0.0, 0.0],
       [ 0.0,  0.0, 1.0, 0.0],
       [-1.0,  1.0, 0.0, 1.0f32]];

    Ok(ImageDisplay{
      program: program,
      vert_buf: vertex_buffer,
      idx_buf: index_buffer,
      view_matrix: display_to_gl
    })
  }

  pub fn draw_image(&mut self, placed_image: &PlacedImage, target: &mut Frame) {
    let mut corner_data = placed_image.corner_data(); // ordered tl, tr, br, bl
    corner_data.swap(2, 3); // make the order tl, tr, br, bl, as needed for the triangle strip
    let verts: Vec<_> = corner_data.iter().map(|&(pos, tex_coord)| Vertex{pos, tex_coord}).collect();

    self.vert_buf.write(&verts);

    let uniforms = uniform! {
      transform: self.view_matrix,
      img: &placed_image.image.texture
    };

    target.draw(&self.vert_buf, &self.idx_buf, &self.program, &uniforms, &Default::default()).expect("Drawing image geometry failed.");
  }
}