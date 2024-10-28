#version 460
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;
layout(set=1, binding=200)         uniform sampler2D textures[];

layout(push_constant, std430) uniform Registers {
  PushConstants pc;
};

void main() {
  payload.t = 0.0;
  payload.emission = pc.uniforms.skycolor.rgb;

  const float PI = 3.14159265359;
  const float INVPI = 1.0 / PI;
  const float INV2PI = 1.0 / (2 * PI);
  float phi = atan(gl_WorldRayDirectionEXT.x, gl_WorldRayDirectionEXT.z);
  float u = ((phi > 0 ? phi : (phi + 2 * PI)) * INV2PI - 0.5f);
  float v = (acos(gl_WorldRayDirectionEXT.y) * INVPI - 0.0f);
  vec2 uv = vec2(u, v);
  if (uv.x > 1.0) uv.x -= 1.0;
  if (uv.y > 1.0) uv.y -= 1.0;
  payload.emission *= pow(texture(textures[pc.skydome], uv).rgb, vec3(2.2));
  // prevent extreme bright spots causing high variance
  payload.emission = clamp(payload.emission, vec3(0.0), vec3(300.0));
}
