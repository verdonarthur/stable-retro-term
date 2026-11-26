@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
struct Crt {
    geometry: vec4<f32>,
    beam: vec4<f32>,       // bloom, burn-in, noise, jitter
    screen: vec4<f32>,     // glow line, curvature, ambient, flicker
    signal: vec4<f32>,     // sync, RGB shift, frame shine, scanlines
    phosphor: vec4<f32>,   // red, green, blue, brightness
    cabinet: vec4<f32>,    // contrast, margin, radius, frame size
    background: vec4<f32>, // background RGB, chroma
    bezel: vec4<f32>,      // frame RGB, saturation lift
}
@group(0) @binding(2) var<uniform> crt: Crt;
@group(0) @binding(3) var history: texture_2d<f32>;

fn sample_at(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(image, linear_sampler, uv, 0.0).rgb;
}

fn noise_at(point: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(point, vec2<f32>(0.06711056, 0.00583715))));
}

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn persistence(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let point = vec2<i32>(pixel.xy);
    let current = textureLoad(image, point, 0).rgb;
    let previous = textureLoad(history, point, 0).rgb;
    let decay = 0.55 + crt.beam.y * 0.42;
    return vec4<f32>(max(current, previous * decay - 0.004), 1.0);
}

@fragment
fn fragment(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(image));
    let p = pixel.xy / size * 2.0 - 1.0;
    var uv = (p * (crt.geometry.x + crt.geometry.y * dot(p, p)) + 1.0) * 0.5;
    let frame = crt.geometry.w;
    let odd = u32(pixel.y) % 2u == 1u;
    if frame < 0.0 {
        if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        return vec4<f32>(sample_at(uv) * vec3<f32>(0.95, 1.0, 1.0) *
            select(1.0, 0.88, odd) * crt.geometry.z, 1.0);
    }

    let radius = crt.cabinet.z * 0.32;
    let bevel = crt.cabinet.w * 0.12;
    let edge = abs(p) - (vec2<f32>(1.0 - radius - bevel));
    let distance = length(max(edge, vec2<f32>(0.0))) + min(max(edge.x, edge.y), 0.0) - radius;
    let antialias = max(fwidth(distance), 0.0001);
    let glass = 1.0 - smoothstep(-antialias, antialias, distance);
    let depth = clamp(1.0 - distance / max(bevel, 0.001), 0.0, 1.0);
    let bevel_light = (0.035 + 0.13 * crt.signal.z * (1.0 - p.y * 0.4)) * (1.0 - depth);
    let frame_color = crt.bezel.rgb * bevel_light;

    let jitter = (noise_at(vec2<f32>(floor(pixel.y), frame)) - 0.5) * crt.beam.w * 5.0;
    let sync_band = smoothstep(0.89, 0.995, sin(uv.y * 13.0 - frame * 0.09));
    uv.x += (jitter + sync_band * crt.signal.x * 24.0) / size.x;

    let inside = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    var color = sample_at(uv);
    if crt.signal.y > 0.0 {
        let shift = vec2<f32>(crt.signal.y * 5.0 / size.x, 0.0);
        color.r = sample_at(uv + shift).r;
        color.b = sample_at(uv - shift).b;
    }
    if crt.beam.x > 0.0 {
        let step = vec2<f32>(3.0) / size;
        let halo = sample_at(uv + vec2<f32>(step.x, 0.0)) + sample_at(uv - vec2<f32>(step.x, 0.0))
            + sample_at(uv + vec2<f32>(0.0, step.y)) + sample_at(uv - vec2<f32>(0.0, step.y));
        color += halo * crt.beam.x * 0.22;
    }
    if crt.beam.y > 0.0 {
        color = max(color, textureSampleLevel(history, linear_sampler, uv, 0.0).rgb * crt.beam.y * 0.7);
    }

    color = select(vec3<f32>(0.0), color, inside);
    let glow = pow(max(sin(uv.y * 6.283 - frame * 0.08), 0.0), 16.0) * crt.screen.x;
    let flicker = 1.0 - noise_at(vec2<f32>(frame, 9.0)) * crt.screen.w * 0.2;
    let grain = (noise_at(pixel.xy + vec2<f32>(frame)) - 0.5) * crt.beam.z * 0.16;
    let brightness = 0.4 + crt.phosphor.w * 1.2;
    let contrast = 0.5 + crt.cabinet.x;
    color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(contrast)) * brightness;
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luminance), color, crt.background.w);
    let tint = mix(crt.phosphor.rgb, vec3<f32>(1.0), crt.bezel.w * 0.5);
    color = (color * (1.0 + glow * 0.22) + glow * 0.05 + grain + crt.screen.z * 0.06)
        * tint * select(1.0, 1.0 - crt.signal.w, odd) * flicker * crt.geometry.z;
    color += crt.background.rgb * (1.0 - clamp(luminance, 0.0, 1.0));
    return vec4<f32>(mix(frame_color, max(color, vec3<f32>(0.0)), glass), 1.0);
}
