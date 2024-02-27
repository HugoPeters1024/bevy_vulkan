#ifndef H_TYPES
#define H_TYPES

#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_scalar_block_layout : require

layout (buffer_reference, scalar, buffer_reference_align = 8) readonly buffer UniformData {
  mat4 inverse_view;
  mat4 inverse_projection;
};

struct HitPayload {
  float t;
  vec3 emission;
};

#endif
