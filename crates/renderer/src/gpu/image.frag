#version 300 es

precision highp float;

// highp, because the default for samplers, lowp, need not keep all 8 bits of
// a channel, and the image has to arrive unchanged.
uniform highp sampler2D u_image;
uniform float u_height;

out vec4 o_color;

void main() {
    // The CPU's rows run top to bottom, OpenGL's bottom to top.
    ivec2 at = ivec2(int(gl_FragCoord.x), int(u_height - gl_FragCoord.y));
    // The bytes are BGRA, uploaded as RGBA because OpenGL ES has no BGRA
    // format without an extension.
    o_color = texelFetch(u_image, at, 0).bgra;
}
