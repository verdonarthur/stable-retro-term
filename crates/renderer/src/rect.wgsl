struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) index: u32, @location(0) bounds: vec4<f32>,
          @location(1) color: vec3<f32>) -> Vertex {
    let corners = array(vec2<f32>(0, 0), vec2<f32>(1, 0), vec2<f32>(0, 1), vec2<f32>(1, 1));
    return Vertex(vec4<f32>(bounds.xy + corners[index] * bounds.zw, 0, 1), color);
}

@fragment
fn fragment(input: Vertex) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1);
}
