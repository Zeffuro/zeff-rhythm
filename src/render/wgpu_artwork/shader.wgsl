struct Parameters {
    uv: vec4<f32>,
    brightness: vec4<f32>,
};

@group(0) @binding(0) var artwork: texture_2d<f32>;
@group(0) @binding(1) var artwork_sampler: sampler;
@group(0) @binding(2) var<uniform> parameters: Parameters;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let vertices = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, 1.0), vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let position = vertices[index];
    let uv = position * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    var out: VertexOut;
    out.position = vec4<f32>(position * parameters.brightness.yz, 0.0, 1.0);
    out.uv = parameters.uv.xy + uv * parameters.uv.zw;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let color = textureSample(artwork, artwork_sampler, input.uv);
    return vec4<f32>(color.rgb * parameters.brightness.x, color.a);
}
