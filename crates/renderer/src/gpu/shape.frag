#version 300 es

// highp, because positions reach thousands of pixels, where mediump would
// round them to whole pixels.
precision highp float;

flat in vec4 v_box;
flat in vec4 v_radii;
flat in vec4 v_cut;
flat in vec4 v_color;
flat in vec4 v_melt;
flat in vec4 v_clip;
flat in vec4 v_wave;

uniform vec2 u_size;
uniform float u_scale;

out vec4 o_color;

// Wave::LENGTH and Wave::SPREAD in shapes/mod.rs.
const float WAVE_LENGTH = 20.0;
const float WAVE_SPREAD = 24.0;
const float PI = 3.14159265358979;

// edge_distance in shapes/mod.rs: how far p is out of the edge's side, waves
// included, divided by the length of the slope's normal.
float edge_distance(vec2 p) {
    float e = dot(v_melt.xy, p) - v_melt.z;
    if (v_wave.y <= 0.0) {
        return e;
    }
    float s = abs(v_melt.y) > 0.5 ? p.x : p.y;
    float r = s - v_wave.x;
    float u = abs(r) - v_wave.z;
    // Within one wavelength, as Wave::height takes it.
    float v = mod(abs(r) - v_wave.w, WAVE_LENGTH);
    float envelope = exp(-(u / WAVE_SPREAD) * (u / WAVE_SPREAD));
    float k = PI * 2.0 / WAVE_LENGTH;
    float crest = 0.5 + 0.5 * cos(k * v);
    float h = v_wave.y * envelope * crest;
    float slope = v_wave.y
        * (envelope * (-2.0 * u / (WAVE_SPREAD * WAVE_SPREAD)) * crest
           + envelope * -0.5 * k * sin(k * v));
    slope *= sign(r);
    return (e - h) / sqrt(1.0 + slope * slope);
}

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
    if (v_melt.w >= 0.0) {
        // The round union with the edge's side, then the clip.
        float e = edge_distance(p);
        float k = v_melt.w;
        d = max(k, min(e, d)) - length(max(vec2(k - e, k - d), 0.0));
        vec2 cq = abs(p - (v_clip.xy + v_clip.zw) * 0.5) - (v_clip.zw - v_clip.xy) * 0.5;
        d = max(d, length(max(cq, 0.0)) + min(max(cq.x, cq.y), 0.0));
    }
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
