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
  uint pull_focus_x;
  uint pull_focus_y;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) buffer FocusData {
  float focal_distance;
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer VertexData {
  Vertex vertices[];
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
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer MaterialData {
  Material materials[];
};

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer BluenoiseData {
  uint bluenoise[];
};


struct HitPayload {
  float t;
  float roughness;
  float metallic;
  bool inside;
  float transmission;
  float refract_index;
  vec4 color;
  vec3 emission;
  vec3 surface_normal;
  vec3 world_normal;
  vec3 absorption;
};


#endif
