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
flat in vec4 v_neck;
flat in vec4 v_more;
flat in vec4 v_bulge;

uniform vec2 u_size;
uniform float u_scale;

out vec4 o_color;

// Wave::LENGTH, Wave::SPREAD and Bulge::SPREAD in shapes/mod.rs.
const float WAVE_LENGTH = 20.0;
const float WAVE_SPREAD = 24.0;
const float BULGE_SPREAD = 20.0;
// TAPER in shapes/mod.rs.
const float TAPER = 24.0;
const float PI = 3.14159265358979;

// taper in shapes/mod.rs: 0 at the extent's ends, 1 from TAPER in, and its
// slope.
vec2 taper(float s) {
    float u = clamp((s - v_bulge.z) / TAPER, 0.0, 1.0);
    float v = clamp((v_bulge.w - s) / TAPER, 0.0, 1.0);
    float su = u * u * (3.0 - 2.0 * u);
    float sv = v * v * (3.0 - 2.0 * v);
    float du = 6.0 * u * (1.0 - u) / TAPER;
    float dv = 6.0 * v * (1.0 - v) / TAPER;
    return vec2(su * sv, du * sv - su * dv);
}

// The side ends at the extent's ends, square, as a box does.
float edge_end(float s, float across) {
    float along = max(v_bulge.z - s, s - v_bulge.w);
    return along > 0.0 && across > 0.0 ? length(vec2(along, across)) : max(along, across);
}

// edge_distance in shapes/mod.rs: how far p is out of the edge's side, its
// waves and bulge included, divided by the length of the slope's normal.
float edge_distance(vec2 p) {
    float e = dot(v_melt.xy, p) - v_melt.z;
    float s = abs(v_melt.y) > 0.5 ? p.x : p.y;
    // 0 for none, as the instance packs them; either may be negative.
    if (v_wave.y == 0.0 && v_bulge.y == 0.0) {
        return edge_end(s, e);
    }
    vec2 t = taper(s);
    float b = (s - v_bulge.x) / BULGE_SPREAD;
    float hb = v_bulge.y * exp(-b * b);
    float sb = hb * -2.0 * b / BULGE_SPREAD;
    if (v_wave.y == 0.0) {
        float slope = sb * t.x + hb * t.y;
        return edge_end(s, (e - hb * t.x) / sqrt(1.0 + slope * slope));
    }
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
    slope = (slope * sign(r) + sb) * t.x + (h + hb) * t.y;
    return edge_end(s, (e - (h + hb) * t.x) / sqrt(1.0 + slope * slope));
}

// Neck::distance in shapes/mod.rs: a curve square to the edge, whose normal
// is v_melt.xy, at both its ends.
float neck_distance(vec2 p) {
    vec2 n = v_melt.xy;
    vec2 d = v_neck.zw - v_neck.xy;
    vec2 q = p - v_neck.xy;
    float rise = dot(d, n);
    vec2 w = d - n * rise;
    bool flat_neck = abs(rise) <= 1e-6;
    float t = flat_neck ? 0.0 : clamp(dot(q, n) / rise, 0.0, 1.0);
    float s = t * t * (3.0 - 2.0 * t);
    vec2 r = q - n * rise * t - w * s;
    float up = dot(r, n);
    float across = abs(r.y * n.x - r.x * n.y);
    float slope = flat_neck ? 0.0 : length(w) * 6.0 * t * (1.0 - t) / abs(rise);
    // The catenary of Neck::half.
    float waist = max(max(v_more.y, v_more.x * 1e-4), 1e-6);
    float half_width = waist * cosh((2.0 * t - 1.0) * acosh(max(v_more.x / waist, 1.0)))
        - (v_more.w - 1.0);
    return length(vec2(across / sqrt(1.0 + slope * slope), up)) - half_width;
}

// smooth_min in shapes/mod.rs.
float smooth_min(float a, float b, float k) {
    if (k <= 0.0) {
        return min(a, b);
    }
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
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
        // The round union with the edge's side, then the neck, which blends
        // no wider than the distance to the edge's nearer end, as room in
        // shapes/mod.rs does, then the clip.
        float e = edge_distance(p);
        float s = abs(v_melt.y) > 0.5 ? p.x : p.y;
        float room = max(min(s - v_bulge.z, v_bulge.w - s), 0.0);
        float k = v_melt.w;
        d = max(k, min(e, d)) - length(max(vec2(k - e, k - d), 0.0));
        if (v_more.w > 0.5) {
            d = smooth_min(d, neck_distance(p), min(v_more.z, room));
        }
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
