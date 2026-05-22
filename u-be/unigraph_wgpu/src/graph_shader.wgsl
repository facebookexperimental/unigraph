// Copyright (c) Meta Platforms, Inc. and affiliates.
struct Selection {
    _padding: vec3<u32>, // 12 (32)
}

struct BasicUniforms {
    aspect_ratio: f32,
    node_size_scale: f32,
    selection_from_point: vec2<f32>,
    selection_to_point: vec2<f32>,
    selection_type: u32,
    background_color: vec4<f32>,
    node_main_color: vec4<f32>,
    node_selected_color: vec4<f32>,
    edge_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> basic_uniforms: BasicUniforms;


struct NodeAttributes {
    position: vec2<f32>,
    adjusted_size: f32,
    flags: u32,
}

const NODE_UNREACHABLE:  u32 = 256u;     // in binary 0001_0000_0000
const NODE_SELECTED:     u32 = 512u;     // in binary 0010_0000_0000
const NODE_FOCUSED:      u32 = 1024u;    // in binary 0100_0000_0000

fn is_flag_set(flags: u32, flag: u32) -> bool {
    return (flags & flag) != 0u;
}

@group(1) @binding(0) var<storage, read> nodeAttributes: array<NodeAttributes>;

struct EdgeAttributes {
    from_node: u32,
    to_node: u32,
}

@group(1) @binding(1) var<storage, read> edgeAttributes: array<EdgeAttributes>;


struct NodeVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) node_radius: f32,
    @location(1) node_color: vec4<f32>,
    @location(2) frag_pos: vec2<f32>,
    @location(3) node_pos: vec2<f32>,
    @location(4) @interpolate(flat) node_attributes_flags: u32,
}


@vertex
fn vs_node(
    @builtin(vertex_index) vertex_idx: u32,
    @builtin(instance_index) instance_index: u32,
) -> NodeVertexOutput {
    // Access the node attributes using the instance index
    let node_attibutes = nodeAttributes[instance_index];

    let node_position = vec2<f32>(node_attibutes.position.x, node_attibutes.position.y);

     // Create a square around each ball position
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, -1.0)
    );

    let aspect_ratio = basic_uniforms.aspect_ratio;

    let adjusted_scale = basic_uniforms.node_size_scale * 0.0002;

    // compute the radius of the node given it's size (which
    // we'll assume is volume). This way very large nodes don't
    // be disproportionately large since the radius will grow
    // logarithmically with size.
    let pi: f32 = 3.141592653589793;
    let radius = sqrt(node_attibutes.adjusted_size / pi);

    let scaled_radius = radius * adjusted_scale;

    let size = scaled_radius * 2.0;
    let aspect_correction = vec2<f32>(aspect_ratio, 1.0);
    let vert_pos = node_position + pos[vertex_idx] * size * aspect_correction;

    var output: NodeVertexOutput;
    output.position = vec4<f32>(vert_pos * vec2<f32>(1.0 / aspect_ratio, 1.0), 0.0, 1.0);
    output.node_radius = scaled_radius;
    output.node_pos = node_position;
    output.node_attributes_flags = node_attibutes.flags;
    // output.node_color = vec3<f32>(0.4654, 0.0091, 0.0480);
    if is_flag_set(node_attibutes.flags, NODE_SELECTED) {
        output.node_color = basic_uniforms.node_selected_color;
    } else {
        output.node_color = basic_uniforms.node_main_color;
    }
    output.frag_pos = vert_pos;

    return output;
}

@fragment
fn fs_node(in: NodeVertexOutput) -> @location(0) vec4<f32> {
  // Calculate distance from fragment to ball center
    let dist = distance(in.frag_pos, in.node_pos);

    if is_flag_set(in.node_attributes_flags, NODE_UNREACHABLE) {
        // If the node is unreachable, discard it
        discard;
    }


    // Discard fragments outside the ball radius
    if dist > in.node_radius {
        discard;
    }

    return in.node_color;
}

struct EdgeVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) points_from_node_attributes_flags: u32,
    @location(1) @interpolate(flat) points_to_node_attributes_flags: u32,
}


@vertex
fn vs_edge(
    @builtin(vertex_index) vertex_idx: u32,
    @builtin(instance_index) instance_index: u32,
) -> EdgeVertexOutput {
    // Access the edge and node attributes using the instance index
    let edge = edgeAttributes[instance_index];
    let from_node_attributes = nodeAttributes[edge.from_node];
    let to_node_attributes = nodeAttributes[edge.to_node];

    // Positions of the nodes
    let from_position = vec2<f32>(from_node_attributes.position.x, from_node_attributes.position.y);
    let to_position = vec2<f32>(to_node_attributes.position.x, to_node_attributes.position.y);

    // Interpolate between the two positions to create the line
    var pos = array<vec2<f32>, 2>(
        from_position,
        to_position
    );

    // Select the vertex position based on vertex_idx
    let vert_pos = pos[vertex_idx];

    var output: EdgeVertexOutput;
    output.position = vec4<f32>(vert_pos * vec2<f32>(1.0 / basic_uniforms.aspect_ratio, 1.0), 0.0, 1.0);
    output.points_from_node_attributes_flags = from_node_attributes.flags;
    output.points_to_node_attributes_flags = to_node_attributes.flags;
    return output;
}

@fragment
fn fs_edge(in: EdgeVertexOutput) -> @location(0) vec4<f32> {
    if is_flag_set(in.points_to_node_attributes_flags, NODE_UNREACHABLE) {
        discard;
    }
    if is_flag_set(in.points_from_node_attributes_flags, NODE_UNREACHABLE) {
        discard;
    }

    return basic_uniforms.edge_color;
}

struct SelectionVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,  // Normalized coordinates within the box (0,0 to 1,1)
}

@vertex
fn vs_box_selection(@builtin(vertex_index) vertex_idx: u32) -> SelectionVertexOutput {
    // Get selection corners and build vertices for a quad (2 triangles)
    let min_x = min(basic_uniforms.selection_from_point.x, basic_uniforms.selection_to_point.x);
    let min_y = min(basic_uniforms.selection_from_point.y, basic_uniforms.selection_to_point.y);

    let max_x = max(basic_uniforms.selection_from_point.x, basic_uniforms.selection_to_point.x);
    let max_y = max(basic_uniforms.selection_from_point.y, basic_uniforms.selection_to_point.y);

    // Define the vertices of the selection box (2 triangles as a strip)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(min_x, min_y),  // Bottom-left
        // vec2<f32>(min_x, -0.7),  // Bottom-left
        vec2<f32>(max_x, min_y),  // Bottom-right
        vec2<f32>(max_x, max_y),  // Top-right
        vec2<f32>(max_x, max_y),  // Top-right
        vec2<f32>(min_x, max_y),  // Top-left
        vec2<f32>(min_x, min_y)   // Bottom-left
    );

    // UVs for determining interior vs border
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 0.0)
    );

    // Apply aspect ratio correction
    let aspect_ratio = basic_uniforms.aspect_ratio;
    let position = positions[vertex_idx];

    var output: SelectionVertexOutput;
    // output.position = vec4<f32>(position.x / aspect_ratio, position.y, 0.0, 1.0);
    output.position = vec4<f32>(position.x, position.y, 0.0, 1.0);
    output.uv = uvs[vertex_idx];

    return output;
}

@fragment
fn fs_box_selection(in: SelectionVertexOutput) -> @location(0) vec4<f32> {
    // Calculate how much UV changes per screen pixel
    let pixel_size_x = abs(dpdx(in.uv.x)) + abs(dpdy(in.uv.x));
    let pixel_size_y = abs(dpdx(in.uv.y)) + abs(dpdy(in.uv.y));

    // Distance to the nearest border in UV space
    let dist_to_edge_x = min(in.uv.x, 1.0 - in.uv.x);
    let dist_to_edge_y = min(in.uv.y, 1.0 - in.uv.y);

    // Check if we're within 1px of any edge
    if dist_to_edge_x < pixel_size_x || dist_to_edge_y < pixel_size_y {
        // Solid white border
        return vec4<f32>(1.0, 1.0, 1.0, 0.8);
    } else {
        // Semi-transparent blue fill
        return vec4<f32>(0.2, 0.4, 0.9, 0.15);
    }
}
