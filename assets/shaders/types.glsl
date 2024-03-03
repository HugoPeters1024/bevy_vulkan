#ifndef H_TYPES
#define H_TYPES

#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_scalar_block_layout : require

struct Vertex {
  vec3 position;
  vec3 normal;
  vec2 texcoord;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer UniformData {
  mat4 inverse_view;
  mat4 inverse_projection;
  uint tick;
  uint accumulate;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer VertexData {
  Vertex vertices[];
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer IndexData {
  uint indices[];
};


struct HitPayload {
  bool hit;
  float t;
  float roughness;
  bool inside;
  vec3 color;
  float transmission;
  vec3 emission;
  float refract_index;
  vec3 world_normal;
  vec3 absorption;
};

struct Material {
  vec4 base_color_factor;
  vec3 base_emissive_factor;
};

#endif
