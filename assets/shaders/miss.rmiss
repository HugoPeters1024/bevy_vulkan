#version 460
#extension GL_EXT_ray_tracing : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  // sky blue
  payload.hit = false;
  payload.color = vec3(0.0, 0.16, 0.4);
}
