@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> params: vec4<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let p = pixel.xy / vec2<f32>(textureDimensions(image)) * 2.0 - 1.0;
    let uv = (p * (params.x + params.y * dot(p, p)) + 1.0) * 0.5;
    if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // The performance capture probe models these fixed gains in sRGB.
    let scanline = select(1.0, 0.88, u32(pixel.y) % 2u == 1u);
    return vec4<f32>(textureSampleLevel(image, linear_sampler, uv, 0.0).rgb *
        vec3<f32>(0.95, 1.0, 1.0) * scanline, 1.0);
}
