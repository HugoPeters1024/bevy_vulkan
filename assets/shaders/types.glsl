#ifndef H_TYPES
#define H_TYPES

#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_scalar_block_layout : require
#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require


struct Vertex {
  vec3 position;
  vec3 normal;
  vec2 texcoord;
};

struct Triangle {
  uint tangent;
  uint normals[3];
  uint uvs[3];
};

vec3 unpackNormal(uint packed) {
  float nx = float(packed >> 16) / 65535.0 * 2.0 - 1.0;
  float ny = float((packed >> 1) & 32767) / 32767.0 * 2.0 - 1.0;
  float nz = sqrt(clamp(1.0 - nx * nx - ny * ny, 0.0, 1.0)) * ((packed & 1) == 1 ? -1.0 : 1.0);
  return vec3(nx, ny, nz);
}

vec2 unpackUv(uint packed) {
  return unpackHalf2x16(packed);
}

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer UniformData {
  mat4 inverse_view;
  mat4 inverse_projection;
  uint tick;
  uint accumulate;
  uint pull_focus_x;
  uint pull_focus_y;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) buffer FocusData {
  float focal_distance;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer VertexData {
  Vertex vertices[];
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer TriangleData {
  Triangle triangles[];
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer IndexData {
  uint indices[];
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer GeometryData {
  uint index_offsets[];
};

struct Material {
  vec4 base_color_factor;
  vec4 base_emissive_factor;
  uint base_color_texture;
  uint base_emissive_texture;
  uint specular_transmission_texture;
  uint metallic_roughness_texture;
  uint normal_texture;
  float specular_transmission_factor;
  float roughness_factor;
  float metallic_factor;
  float refract_index;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer MaterialData {
  Material materials[];
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer BluenoiseData {
  uint bluenoise[];
};


struct HitPayload {
  float t;
  float refract_index;
  // r = roughness, m = metallic, t = transmission, i = inside
  int r_m_t_i;
  vec4 color;
  vec3 emission;
  vec4 surface_and_world_normal;
  vec3 absorption;
};

struct PushConstants {
  UniformData uniforms;
  MaterialData materials;
  BluenoiseData bluenoise2;
  FocusData focus;
  uint64_t skydome;
  vec4 skycolor;
};

void hitPayloadSetRoughness(inout HitPayload p, float r) {
  int v = int(r *255.0) % 256;
  p.r_m_t_i = (p.r_m_t_i & 0x00FFFFFF) | (v << 24);
}

float hitPayloadGetRoughness(inout HitPayload p) {
  int v = (p.r_m_t_i >> 24) % 256;
  return v / 255.0;
}

void hitPayloadSetMetallic(inout HitPayload p, float m) {
  int v = int(m *255.0) % 256;
  p.r_m_t_i = (p.r_m_t_i & 0xFF00FFFF) | (v << 16);
}

float hitPayloadGetMetallic(inout HitPayload p) {
  int v = (p.r_m_t_i >> 16) % 256;
  return v / 255.0;
}

void hitPayloadSetTransmission(inout HitPayload p, float t) {
  int v = int(t * 255.0) % 256;
  p.r_m_t_i = (p.r_m_t_i & 0xFFFF00FF) | (v << 8);
}

float hitPayloadGetTransmission(inout HitPayload p) {
  int v = (p.r_m_t_i >> 8) % 256;
  return v / 255.0;
}

void hitPayloadSetInside(inout HitPayload p, bool i) {
  p.r_m_t_i = (p.r_m_t_i & 0xFFFFFF00) | (i ? 0xFF : 0);
}

bool hitPayloadGetInside(inout HitPayload p) {
  int v = p.r_m_t_i % 256;
  return v == 0xFF;
}


// Returns +/- 1
vec2 signNotZero( vec2 v )
{
    return vec2((v.x >= 0.0) ? +1.0 : -1.0, (v.y >= 0.0) ? +1.0 : -1.0);
}

// Assume normalized input. Output is on [-1, 1] for each component.
vec2 float32x3_to_oct( in vec3 v )
{
    // Project the sphere onto the octahedron, and then onto the xy plane
    vec2 p = v.xy * (1.0 / (abs(v.x) + abs(v.y) + abs(v.z)));
    // Reflect the folds of the lower hemisphere over the diagonals
    return (v.z <= 0.0) ? ((1.0 - abs(p.yx)) * signNotZero(p)) : p;
}

vec3 oct_to_float32x3(in vec2 e )
{
    vec3 v = vec3(e.xy, 1.0 - abs(e.x) - abs(e.y));
    if (v.z < 0) v.xy = (1.0 - abs(v.yx)) * signNotZero(v.xy);
    return normalize(v);
}

vec4 pack2_normals(in vec3 lhs, in vec3 rhs) {
  return vec4(float32x3_to_oct(lhs), float32x3_to_oct(rhs));
}

#endif
