#version 300 es

// Two triangles over the whole framebuffer, from the vertex index alone.
void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1)) * 2.0 - 1.0;
    gl_Position = vec4(corner, 0.0, 1.0);
}
