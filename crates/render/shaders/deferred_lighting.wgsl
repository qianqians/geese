// Deferred+ lighting pass：全屏 quad，从 G-Buffer + depth 还原世界位置并按 cluster 着色。
// 与 [pbr_common.wgsl](file:///Users/qianqians/Documents/geese/crates/render/shaders/pbr_common.wgsl)
// 共享 PBR BRDF 与 cluster 工具。

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> lights: LightStorage;
@group(0) @binding(2) var<uniform> cluster: ClusterUniform;
@group(0) @binding(3) var<storage, read> cluster_bitmasks: array<u32, TOTAL_CLUSTERS>;

@group(1) @binding(0) var gbuffer_base_metallic: texture_2d<f32>;
@group(1) @binding(1) var gbuffer_normal_roughness: texture_2d<f32>;
@group(1) @binding(2) var gbuffer_emissive_occlusion: texture_2d<f32>;
@group(1) @binding(3) var gbuffer_depth: texture_depth_2d;
@group(1) @binding(4) var gbuffer_sampler: sampler;

struct FullscreenVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// 用 vertex_index 生成全屏三角形（覆盖 NDC [-1,1]^2），无需 vertex buffer。
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> FullscreenVertexOutput {
    var output: FullscreenVertexOutput;
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    let uv = vec2<f32>(x, y);
    output.uv = uv;
    output.clip_position = vec4<f32>(uv * 2.0 - vec2<f32>(1.0), 0.0, 1.0);
    return output;
}

fn world_position_from_depth(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv * 2.0 - vec2<f32>(1.0), depth, 1.0);
    // wgpu 使用 Y 朝下的 framebuffer 但 NDC Y 朝上，因此需要翻转 Y
    let ndc_flipped = vec4<f32>(ndc.x, -ndc.y, ndc.z, ndc.w);
    let world = camera.inverse_view_projection * ndc_flipped;
    return world.xyz / world.w;
}

@fragment
fn fs_main(input: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uv;
    let frag_coord = vec2<i32>(input.clip_position.xy);
    let depth = textureLoad(gbuffer_depth, frag_coord, 0);
    if (depth >= 1.0) {
        // 远平面：天空区域，输出 ambient + emissive=0
        return vec4<f32>(lights.ambient.rgb, 1.0);
    }

    let base_metallic = textureSample(gbuffer_base_metallic, gbuffer_sampler, uv);
    let normal_roughness = textureSample(gbuffer_normal_roughness, gbuffer_sampler, uv);
    let emissive_occlusion = textureSample(gbuffer_emissive_occlusion, gbuffer_sampler, uv);

    let base_color = base_metallic.rgb;
    let metallic = base_metallic.a;
    let n = normalize(normal_roughness.xyz * 2.0 - vec3<f32>(1.0));
    let roughness = clamp(normal_roughness.w, 0.04, 1.0);
    let emissive = emissive_occlusion.rgb;
    let occlusion = emissive_occlusion.a;

    let world_position = world_position_from_depth(uv, depth);
    let v = normalize(camera.camera_position.xyz - world_position);
    let f0 = mix(vec3<f32>(0.04), base_color, metallic);

    let view_z = length(camera.camera_position.xyz - world_position);
    let frag_xy = input.clip_position.xy;
    let cluster_index = cluster_index_from_screen(frag_xy, view_z, cluster);
    let bitmask = cluster_bitmasks[min(cluster_index, TOTAL_CLUSTERS - 1u)];
    let count = min(lights.count.x, MAX_LIGHTS);

    var color = vec3<f32>(0.0);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        if ((bitmask & (1u << i)) == 0u) {
            continue;
        }
        color = color + shade_light(
            lights.lights[i],
            world_position,
            n,
            v,
            base_color,
            metallic,
            roughness,
            f0,
        );
    }

    color = color + lights.ambient.rgb * base_color * occlusion;
    color = color + emissive;

    return vec4<f32>(color, 1.0);
}
