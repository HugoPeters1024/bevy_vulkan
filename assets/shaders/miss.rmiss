#version 460
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;
layout(set=1, binding=42)         uniform sampler2D textures[];

layout(push_constant, std430) uniform Registers {
  UniformData uniforms;
  MaterialData materials;
  FocusData focus;
  uint skydome;
  uint _padding;
};

void main() {
  payload.t = 0.0;
  payload.emission = vec3(1.0) * 1;
  if (skydome != 0xFFFFFFFF) {
    const float PI = 3.14159265359;
    const float INVPI = 1.0 / PI;
    const float INV2PI = 1.0 / (2 * PI);
    float phi = atan(gl_WorldRayDirectionEXT.x, gl_WorldRayDirectionEXT.z);
    float u = ((phi > 0 ? phi : (phi + 2 * PI)) * INV2PI - 0.5f);
    float v = (acos(gl_WorldRayDirectionEXT.y) * INVPI - 0.0f);
    vec2 uv = vec2(u, v);
    payload.emission = pow(texture(textures[skydome], uv).rgb, vec3(2.2));
  }
}
