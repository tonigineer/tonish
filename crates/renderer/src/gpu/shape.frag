#version 300 es

// highp, because positions reach thousands of pixels, where mediump would
// round them to whole pixels.
precision highp float;

flat in vec4 v_box;
flat in vec4 v_radii;
flat in vec4 v_cut;
flat in vec4 v_color;

uniform vec2 u_size;
uniform float u_scale;

out vec4 o_color;

// Sdf::distance in shapes/mod.rs is the same function, which the tests hold
// against every shape's own distance. A change here needs the same change
// there. Not named distance, which is a built-in that Mesa, unlike NVIDIA,
// refuses to have redefined.
float signed_distance(vec2 p) {
    vec2 c = p - v_box.xy;
    // The corner's own radius; y grows downwards, so a positive c.y is the
    // bottom half.
    float r = c.x > 0.0 ? (c.y > 0.0 ? v_radii.z : v_radii.y)
                        : (c.y > 0.0 ? v_radii.w : v_radii.x);
    vec2 q = abs(c) - v_box.zw + r;
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
    d = v_cut.z > 0.0 ? max(d, v_cut.z - length(p - v_cut.xy)) : d;
    return v_cut.w > 0.0 ? abs(d) - v_cut.w * 0.5 : d;
}

void main() {
    // From the pixel's centre rather than an interpolated position, so that
    // the point is exactly the one the CPU rasteriser samples.
    vec2 p = vec2(gl_FragCoord.x, u_size.y - gl_FragCoord.y) / u_scale;
    // The CPU rasteriser's coverage: a ramp one physical pixel wide.
    float alpha = clamp(0.5 - signed_distance(p) * u_scale, 0.0, 1.0) * v_color.a;
    // Premultiplied, as the blend function and the compositor expect.
    o_color = vec4(v_color.rgb * alpha, alpha);
}
