#version 300 es

// One instance per shape. The quad covers its box, padded by half the
// outline and by the pixel the antialiased fringe reaches past the edge, as
// the CPU rasteriser pads its bounds.

layout(location = 0) in vec4 a_box;   // centre and half size, logical pixels
layout(location = 1) in vec2 a_shape; // corner radius and outline width
layout(location = 2) in vec3 a_cut;   // centre and radius of the circle cut out
layout(location = 3) in vec4 a_color; // straight alpha

uniform vec2 u_size; // the framebuffer, physical pixels
uniform float u_scale;

flat out vec4 v_box;
flat out vec2 v_shape;
flat out vec3 v_cut;
flat out vec4 v_color;

void main() {
    // The corners come from the vertex index, so the quad needs no vertex
    // buffer of its own.
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1)) * 2.0 - 1.0;
    vec2 p = a_box.xy + corner * (a_box.zw + a_shape.y * 0.5 + 1.0 / u_scale);
    vec2 ndc = p * u_scale / u_size * 2.0 - 1.0;
    // Logical y grows downwards, OpenGL's upwards.
    gl_Position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
    v_box = a_box;
    v_shape = a_shape;
    v_cut = a_cut;
    v_color = a_color;
}
