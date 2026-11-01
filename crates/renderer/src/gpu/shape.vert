#version 300 es

// One instance per shape. The quad covers its box, padded by half the
// outline and by the pixel the antialiased fringe reaches past the edge, as
// the CPU rasteriser pads its bounds.

layout(location = 0) in vec4 a_box;   // centre and half size, logical pixels
layout(location = 1) in vec4 a_radii; // top left, top right, bottom right, bottom left
layout(location = 2) in vec4 a_cut;   // the circle cut out, centre and radius; outline width
layout(location = 3) in vec4 a_color; // straight alpha
layout(location = 4) in vec4 a_melt;  // the edge's normal and offset, the corners' radius; radius < 0: none
layout(location = 5) in vec4 a_clip;  // the melted shape's clip, as its corners
layout(location = 6) in vec4 a_wave;  // waves on the edge: centre, amplitude, group, phase

uniform vec2 u_size; // the framebuffer, physical pixels
uniform float u_scale;

flat out vec4 v_box;
flat out vec4 v_radii;
flat out vec4 v_cut;
flat out vec4 v_color;
flat out vec4 v_melt;
flat out vec4 v_clip;
flat out vec4 v_wave;

void main() {
    // The corners come from the vertex index, so the quad needs no vertex
    // buffer of its own.
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1)) * 2.0 - 1.0;
    // A melted shape reaches into its edge's side as far as its clip, past
    // its box.
    vec2 centre = a_melt.w < 0.0 ? a_box.xy : (a_clip.xy + a_clip.zw) * 0.5;
    vec2 half_size = a_melt.w < 0.0 ? a_box.zw : (a_clip.zw - a_clip.xy) * 0.5;
    vec2 p = centre + corner * (half_size + a_cut.w * 0.5 + 1.0 / u_scale);
    vec2 ndc = p * u_scale / u_size * 2.0 - 1.0;
    // Logical y grows downwards, OpenGL's upwards.
    gl_Position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
    v_box = a_box;
    v_radii = a_radii;
    v_cut = a_cut;
    v_color = a_color;
    v_melt = a_melt;
    v_clip = a_clip;
    v_wave = a_wave;
}
